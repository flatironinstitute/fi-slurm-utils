use std::collections::{HashMap, HashSet};
use fi_slurm::{jobs::{get_jobs, print_accounts, AccountJobUsage, FilterMethod, JobState}, nodes::get_nodes};
use users::get_current_username;
use fi_slurm_db::acct::{TresMax, get_tres_info};
use fi_slurm::nodes::Node;
use crate::build_node_to_job_map;

pub fn print_limits(qos_name: Option<&String>) {

    let name = qos_name.cloned().unwrap_or_else(|| {
        get_current_username().unwrap_or_else(|| {
            eprintln!("Could not find user information: ensure that the running user is not deleted while the program is running");
            "".into()
        }).to_string_lossy().into_owned() // handle the rare None case
    });

    let accounts = get_tres_info(Some(name.clone())).first().unwrap().clone(); //None case tries to get name from OS

    let jobs_collection = get_jobs().unwrap();

    let mut user_usage: Vec<AccountJobUsage> = Vec::new();
    let mut center_usage: Vec<AccountJobUsage> = Vec::new();


    // get the user ACCT, other, SCC, etc
    // and further use that


    //CENTER LIMITS ({acct})
    accounts.iter().for_each(|a| {
        let group = a.clone().name;

        let center_jobs = jobs_collection.clone()
            .filter_by(FilterMethod::Partition(group.clone()));
            



            //...
            //.filter_by(FilterMethod::Account(group.clone()));

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

    println!("\nUser Limits");
    print_accounts(user_usage);

    println!("\nCenter Limits");
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

    let jobs_collection = get_jobs().unwrap();

    let nodes_collection = get_nodes().unwrap();

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

    println!("{}", map.len());

    let mut sorted_scores: Vec<(&String, &(u32, u32))> = map.iter().collect();

    sorted_scores.sort_by(|a, b| b.1.cmp(a.1));

    for (position, (user, score)) in sorted_scores.iter().enumerate().take(top_n) {
        let rank = position + 1;
        let padding = if rank > 9 { "" } else {" "}; // just valid for the first 100
        // let (initial, surname) = user.split_at_checked(1).unwrap_or(("Dr", "Evil"));
        println!("{}. {} {} is using {} nodes and {} cores", rank, padding, user, score.0, score.1);
    }
}
