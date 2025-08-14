use std::collections::{HashMap, HashSet};
use fi_slurm::{jobs::{get_jobs, print_accounts, AccountJobUsage, FilterMethod, JobState, SlurmJobs, build_node_to_job_map}, nodes::get_nodes};
use users::get_current_username;
use fi_slurm_db::acct::{TresMax, get_tres_info};
use fi_slurm::parser::parse_slurm_hostlist;

pub fn print_limits(qos_name: Option<&String>) {

    let name = qos_name.cloned().unwrap_or_else(|| {
        get_current_username().unwrap_or_else(|| {
            eprintln!("Could not find user information: ensure that the running user is not deleted while the program is running");
            "".into()
        }).to_string_lossy().into_owned() // handle the rare None case
    });

    let (user_acct, accounts_to_process) = get_tres_info(Some(name.clone())); //None case tries to get name from OS
    
    let accounts = accounts_to_process.first().unwrap().clone();

    let mut jobs_collection = get_jobs().unwrap();

    jobs_collection.jobs.retain(|&_, job| {
        job.job_state == JobState::Running
    });

    let mut user_usage: Vec<AccountJobUsage> = Vec::new();
    let mut center_usage: Vec<AccountJobUsage> = Vec::new();

    //CENTER LIMITS ({acct})
    accounts.iter().for_each(|a| {
        let group = a.clone().name;

        let center_jobs = jobs_collection.clone()
            .filter_by(FilterMethod::Partition(group.clone()))
            .filter_by(FilterMethod::Account(user_acct.clone())); 

        let center_gres_count = center_jobs.get_gres_total();

        let (center_nodes, center_cores) = center_jobs.get_resource_use();

        let user_jobs = jobs_collection.clone()
            .filter_by(FilterMethod::Partition(group.clone()))
            .filter_by(FilterMethod::UserName(name.clone()));

        let (user_nodes, user_cores) = user_jobs.get_resource_use();
        let user_gres_count = user_jobs.get_gres_total();

        let user_tres_max = TresMax::new(a.max_tres_per_user.clone().unwrap_or("".to_string()));
        let user_max_nodes = user_tres_max.max_nodes.unwrap_or(0);
        let user_max_cores = user_tres_max.max_cores.unwrap_or(0);
        let user_max_gres = user_tres_max.max_gpus.unwrap_or(0);

        let center_tres_max = TresMax::new(a.max_tres_per_group.clone().unwrap_or("".to_string()));
        let center_max_nodes = center_tres_max.max_nodes.unwrap_or(0);
        let center_max_cores = center_tres_max.max_cores.unwrap_or(0);
        let center_max_gres = center_tres_max.max_gpus.unwrap_or(0);

        user_usage.push(AccountJobUsage::new(
            &group, 
            user_nodes, 
            user_cores, 
            user_gres_count,
            user_max_nodes, 
            user_max_cores, 
            user_max_gres,
        ));
        center_usage.push(AccountJobUsage::new(
            &group, 
            center_nodes, 
            center_cores, 
            center_gres_count,
            center_max_nodes, 
            center_max_cores, 
            center_max_gres,
        ));

    });

    // a special edge case to deal with the fact that we need to get the QOS limits for the gen
    // partition from the inter QOS
    let mut gen_acc: Option<AccountJobUsage> = None;
    let mut inter_acc: Option<AccountJobUsage> = None;

    // retain the elements other than gen and inter, and 
    // store those elements above before removing them
    user_usage.retain(|job_usage| {
        match job_usage.account.as_str() {
            "gen" => {
                // We found "gen". Clone it to take ownership, then return `false` to remove it.
                gen_acc = Some(job_usage.clone());
                false
            },
            "inter" => {
                // We found "inter". Clone it, then return `false` to remove it.
                inter_acc = Some(job_usage.clone());
                false
            },
            _ => {
                // This is not "gen" or "inter", so keep it in the vector.
                true
            }
        }
    });

    // check if both items were extracted
    // handle the case where we failed to get both with a warning?
    if let (Some(gen_bla), Some(inter)) = (&gen_acc, &inter_acc) {
        // create composite element from their combination
        let gen_inter = AccountJobUsage::new(
            "gen", 
            gen_bla.nodes,
            gen_bla.cores,
            gen_bla.gpus,
            inter.max_nodes,
            inter.max_cores,
            inter.max_gpus
        );

        user_usage.insert(0, gen_inter);
    } else {
        // if we only found one, add it back in
        if let Some(gen_bla) = gen_acc {
            user_usage.insert(0, gen_bla);
        } else if let Some (inter) = inter_acc {
            user_usage.push(inter); // doesn't need to be at the top
        } else {
            // the case where neither were present, we just pass a user warning
            println!("WARNING: Could not find both 'gen' and 'inter' accounts. No composite account was created.");
        };
    }
    
    // only retain those lines for which there are some non-zero quantities
    user_usage.retain(|user| {
        ![ user.nodes, 
            user.cores, 
            user.gpus, 
            user.max_nodes, 
            user.max_cores, 
            user.max_gpus,
        ].iter().all(|i| *i==0)
    });

    // only retain those lines for which there are some non-zero LIMITS 
    center_usage.retain(|center| {
        ![ center.max_nodes, 
            center.max_cores, 
            center.max_gpus,
        ].iter().all(|i| *i==0)
    });

    println!("\nUser Limits");
    print_accounts(user_usage);

    println!("\nCenter Limits ({})", user_acct);
    print_accounts(center_usage);
}

pub fn leaderboard(top_n: usize) {
    let mut map: HashMap<String, (u32, u32)> = HashMap::new();

    let jobs_collection = get_jobs().unwrap();

    jobs_collection.jobs.iter().for_each(|(_, job)| {
        if job.job_state == JobState::Running {
            let usage = map.entry(job.user_name.clone()).or_insert((0, 0)); //(job.user_name, (job.num_nodes, job.num_cpus))

            usage.0 += job.num_nodes;
            usage.1 += job.num_cpus;
        }
    });

    let mut sorted_scores: Vec<(&String, &(u32, u32))> = map.iter().collect();

    sorted_scores.sort_by(|a, b| b.1.cmp(a.1));

    for (position, (user, score)) in sorted_scores.iter().enumerate().take(top_n) {
        let rank = position + 1;
        let padding = if rank > 9 { "" } else {" "}; // just valid for the first 100
        println!("{}. {} {} is using {} nodes and {} cores", rank, padding, user, score.0, score.1);
    }
}

pub fn leaderboard_feature(top_n: usize, features: Vec<String>) {
    let mut map: HashMap<String, (u32, u32)> = HashMap::new();

    let mut jobs_collection = get_jobs().unwrap();

    let nodes_collection = get_nodes().unwrap();

    enrich_jobs_with_node_ids(&mut jobs_collection, &nodes_collection.name_to_id);

    // keys are node host ids, values are job ids running on those nodes
    let node_to_job_map = build_node_to_job_map(&jobs_collection);

    let features_set: HashSet<String> = HashSet::from_iter(features.iter().cloned());

    let filtered_job_ids: Vec<u32> = nodes_collection.nodes.iter()
        .filter(|node| node.features.iter().any(|item| features_set.contains(item)))
        .filter_map(|node| node_to_job_map.get(&node.id))
        .flatten().cloned().collect();

    let filtered_jobs_collection = jobs_collection
        .filter_by(FilterMethod::JobIds(filtered_job_ids));


    filtered_jobs_collection.jobs.iter().for_each(|(_, job)| {
        if job.job_state == JobState::Running {
            let usage = map.entry(job.user_name.clone()).or_insert((0, 0)); //(job.user_name, (job.num_nodes, job.num_cpus))

            usage.0 += job.num_nodes;
            usage.1 += job.num_cpus;
        }
    });

    let mut sorted_scores: Vec<(&String, &(u32, u32))> = map.iter().collect();

    sorted_scores.sort_by(|a, b| b.1.cmp(a.1));

    for (position, (user, score)) in sorted_scores.iter().enumerate().take(top_n) {
        let rank = position + 1;
        let padding = if rank > 9 { "" } else {" "}; // just valid for the first 100
        // let (initial, surname) = user.split_at_checked(1).unwrap_or(("Dr", "Evil"));
        println!("{}. {} {} is using {} nodes and {} cores", rank, padding, user, score.0, score.1);
    }
}


pub fn enrich_jobs_with_node_ids(
    slurm_jobs: &mut SlurmJobs, 
    name_to_id: &HashMap<String, usize>,
) {
    // iterate mutably over the jobs vector
    for job in &mut slurm_jobs.jobs.values_mut() {
        if job.raw_hostlist.is_empty() {
            continue;
        }

        // parse the hostlist string
        let expanded_nodes = parse_slurm_hostlist(&job.raw_hostlist);

        // convert names to IDs and populate the job's node_ids vector
        job.node_ids.reserve(expanded_nodes.len());
        for node_name in expanded_nodes {
            if let Some(&id) = name_to_id.get(&node_name) {
                job.node_ids.push(id);
            }
        }

        // free the memory from the raw string if it's no longer needed.
        job.raw_hostlist.clear();
        job.raw_hostlist.shrink_to_fit();
    }
}

