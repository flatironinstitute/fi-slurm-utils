use colored::Colorize;
use fi_slurm::parser::parse_slurm_hostlist;
use fi_slurm::{
    jobs::{
        AccountJobUsage, AcctUsageWidths, FilterMethod, JobState, SlurmJobs, build_node_to_job_map,
        get_jobs, print_accounts,
    },
    nodes::get_nodes,
};
use fi_slurm_db::acct::{TresMax, get_tres_info};
use std::collections::{HashMap, HashSet};

const ALWAYS_SHOW: [&str; 2] = ["preempt", "gpupreempt"];

pub fn print_limits(name: &str, show_all: bool) {
    let (user_acct, partitions) = get_tres_info(Some(name.to_string())).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    let mut jobs_collection = get_jobs().unwrap();

    jobs_collection
        .jobs
        .retain(|&_, job| job.job_state == JobState::Running);

    let mut user_usage: Vec<AccountJobUsage> = Vec::new();
    let mut center_usage: Vec<AccountJobUsage> = Vec::new();

    //CENTER LIMITS ({acct})
    partitions.iter().for_each(|a| {
        let group = a.partition.clone();

        // the partition is what a user submits to, so it labels the row; under -v the QOS
        // the limits actually come from gets a column, since it is not always the same name
        let qos = if show_all {
            Some(a.qos.clone().unwrap_or_else(|| "-".to_string()))
        } else {
            None
        };

        let center_jobs = jobs_collection
            .clone()
            .filter_by(FilterMethod::Partition(group.clone()))
            .filter_by(FilterMethod::Account(user_acct.clone()));

        let center_gres_count = center_jobs.get_gres_total();

        let (center_nodes, center_cores) = center_jobs.get_resource_use();
        let center_job_count = center_jobs.jobs.len() as u32;

        let user_jobs = jobs_collection
            .clone()
            .filter_by(FilterMethod::Partition(group.clone()))
            .filter_by(FilterMethod::UserName(name.to_string()));

        let (user_nodes, user_cores) = user_jobs.get_resource_use();
        let user_gres_count = user_jobs.get_gres_total();
        let user_job_count = user_jobs.jobs.len() as u32;

        let user_tres_max = TresMax::new(a.max_tres_per_user.clone().unwrap_or("".to_string()));
        let center_tres_max = TresMax::new(a.max_tres_per_group.clone().unwrap_or("".to_string()));

        user_usage.push(AccountJobUsage {
            account: group.clone(),
            qos: qos.clone(),
            cores: user_cores,
            nodes: user_nodes,
            gpus: user_gres_count,
            jobs: user_job_count,
            max_cores: user_tres_max.max_cores,
            max_nodes: user_tres_max.max_nodes,
            max_gpus: user_tres_max.max_gpus,
            max_jobs: a.max_jobs_per_user,
        });
        center_usage.push(AccountJobUsage {
            account: group.clone(),
            qos: qos.clone(),
            cores: center_cores,
            nodes: center_nodes,
            gpus: center_gres_count,
            jobs: center_job_count,
            max_cores: center_tres_max.max_cores,
            max_nodes: center_tres_max.max_nodes,
            max_gpus: center_tres_max.max_gpus,
            // MaxJobsPU is a per-user limit, so the center has no counterpart to show
            max_jobs: None,
        });
    });

    if !show_all {
        // keep the lines that have either usage or a limit to report, plus the ones we
        // specify should never be hidden
        user_usage.retain(|user| {
            let has_usage = [user.cores, user.nodes, user.gpus, user.jobs]
                .iter()
                .any(|&n| n != 0);
            let has_limit = [user.max_cores, user.max_nodes, user.max_gpus, user.max_jobs]
                .iter()
                .any(Option::is_some);

            ALWAYS_SHOW.contains(&user.account.as_str()) || has_usage || has_limit
        });

        // only retain those lines that have a group limit to report
        center_usage.retain(|center| {
            [center.max_nodes, center.max_cores, center.max_gpus]
                .iter()
                .any(Option::is_some)
        });
    }

    // Sort both by account name
    user_usage.sort_by(|a, b| a.account.cmp(&b.account));
    center_usage.sort_by(|a, b| a.account.cmp(&b.account));

    // shared widths so the two reports line up column-for-column
    let widths = AcctUsageWidths::default()
        .measure(&user_usage)
        .measure(&center_usage);

    // the bare partition names need no heading; the annotated table does
    let label_title = if show_all { "PARTITION" } else { "" };

    let table_width = widths.table_width(label_title);

    print_heading(&format!("User Limits ({name})"), table_width);
    print_accounts(&user_usage, &widths, label_title);

    print_heading(&format!("Center Limits ({user_acct})"), table_width);
    print_accounts(&center_usage, &widths, label_title);
}

/// Centres a section heading in a rule spanning `table_width`, both so it does not sit off to
/// the left of right-aligned column headers, and to match the full-width `═` rules fi-nodes
/// divides its reports with
fn print_heading(heading: &str, table_width: usize) {
    let heading = format!(" {heading} ");
    let rule = table_width.saturating_sub(heading.len());
    let left = rule / 2;

    println!(
        "\n{}{}{}",
        "═".repeat(left),
        heading.as_str().bold(),
        "═".repeat(rule - left)
    );
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
        println!(
            "{:>2}. {:<12} is using {:>4} nodes and {:>5} cores",
            rank, user, score.0, score.1
        );
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

    let filtered_job_ids: Vec<u32> = nodes_collection
        .nodes
        .iter()
        .filter(|node| node.features.iter().any(|item| features_set.contains(item)))
        .filter_map(|node| node_to_job_map.get(&node.id))
        .flatten()
        .cloned()
        .collect();

    let filtered_jobs_collection =
        jobs_collection.filter_by(FilterMethod::JobIds(filtered_job_ids));

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
        // let (initial, surname) = user.split_at_checked(1).unwrap_or(("Dr", "Evil"));
        println!(
            "{:>2}. {:<12} is using {:>4} nodes and {:>5} cores",
            rank, user, score.0, score.1
        );
    }
}

pub fn enrich_jobs_with_node_ids(slurm_jobs: &mut SlurmJobs, name_to_id: &HashMap<String, usize>) {
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
