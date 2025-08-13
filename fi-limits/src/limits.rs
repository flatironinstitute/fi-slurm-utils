use std::collections::{HashMap, HashSet};
use fi_slurm::{jobs::{get_jobs, print_accounts, AccountJobUsage, FilterMethod, JobState, SlurmJobs}, nodes::get_nodes};
use users::get_current_username;
use fi_slurm_db::acct::{TresMax, get_tres_info};
use fi_slurm::nodes::Node;
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
    // from this, we need to get what account, like SCC, CCA, etc, the user is a part of 

    let jobs_collection = get_jobs().unwrap();

    let mut user_usage: Vec<AccountJobUsage> = Vec::new();
    let mut center_usage: Vec<AccountJobUsage> = Vec::new();


    // get the user ACCT, other, SCC, etc
    // and further use that



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

        // if all substantive quantities are 0/-, don't push

        if !vec![
            user_nodes, 
            user_cores, 
            user_gres_count, 
            user_max_nodes, 
            user_max_cores, 
            user_max_gres,
        ].iter().all(|i| i==0) {
            user_usage.push(AccountJobUsage::new(
                &group, 
                user_nodes, 
                user_cores, 
                user_gres_count,
                user_max_nodes, 
                user_max_cores, 
                user_max_gres,
            ));
        };
        if !vec![ 
            center_nodes, 
            center_cores, 
            center_gres_count,
            center_max_nodes, 
            center_max_cores, 
            center_max_gres,
        ].iter().all(|i| i==0) {
            center_usage.push(AccountJobUsage::new(
                &group, 
                center_nodes, 
                center_cores, 
                center_gres_count,
                center_max_nodes, 
                center_max_cores, 
                center_max_gres,
            ));
        };
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
        // let (initial, surname) = user.split_at_checked(1).unwrap_or(("Dr", "Evil"));
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

    // now process this 
    // goal: collect only the jobs where at least one of its allocated nodes has at least one of
    // the stated features
    // we can just go through the nodes, collect all the ids where this is the case, and then use
    // this to filter the jobs


    let filtered_nodes: Vec<&Node> = if features.len() == 1 {
        let feature = features.first().unwrap();
        nodes_collection.nodes.iter().filter(|node| {
            node.features.contains(feature)
        }).collect()
    } else {
        let features_set: HashSet<String> = HashSet::from_iter(features.iter().cloned());

        nodes_collection.nodes.iter().filter(|node| {
            node.features.iter().any(|item| features_set.contains(item))
        }).collect()
    };

    
    let mut filtered_job_ids: Vec<u32> = Vec::new();

    filtered_nodes.iter().for_each(|node| {
        if let Some(jobs) = node_to_job_map.get(&node.id) {
            filtered_job_ids.extend(jobs)
        }
    });
    
    // final stretch, we filter those jobs whose ids are in here
    // is there a way to do this with fewer filtering steps?

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


fn build_node_to_job_map(slurm_jobs: &SlurmJobs) -> HashMap<usize, Vec<u32>> {
    let mut node_to_job_map: HashMap<usize, Vec<u32>> = HashMap::new();

    for job in slurm_jobs.jobs.values() {
        if job.job_state != JobState::Running || job.node_ids.is_empty() {
            continue;
        }
        for &node_id in &job.node_ids {
            node_to_job_map.entry(node_id).or_default().push(job.job_id);
        }
    }
    node_to_job_map
}

pub fn enrich_jobs_with_node_ids(
    slurm_jobs: &mut SlurmJobs, // Needs to be mutable to modify the jobs
    name_to_id: &HashMap<String, usize>,
) {
    // We iterate mutably over the jobs vector
    for job in &mut slurm_jobs.jobs.values_mut() {
        if job.raw_hostlist.is_empty() {
            continue;
        }

        // 1. Parse the hostlist string
        let expanded_nodes = parse_slurm_hostlist(&job.raw_hostlist);

        // 2. Convert names to IDs and populate the job's node_ids vector
        //    Pre-allocating capacity is a small extra optimization.
        job.node_ids.reserve(expanded_nodes.len());
        for node_name in expanded_nodes {
            if let Some(&id) = name_to_id.get(&node_name) {
                job.node_ids.push(id);
            }
        }

        // 3. (Optional) Free the memory from the raw string if it's no longer needed.
        job.raw_hostlist.clear();
        job.raw_hostlist.shrink_to_fit();
    }
}

