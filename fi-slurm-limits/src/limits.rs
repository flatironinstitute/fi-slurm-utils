use colored::Colorize;
use fi_slurm::assoc_mgr::{QosLimits, load as load_assoc_mgr};
use fi_slurm::parser::parse_slurm_hostlist;
use fi_slurm::partitions::get_partitions;
use fi_slurm::{
    jobs::{
        AccountJobUsage, AcctUsageWidths, FilterMethod, JobState, SlurmJobs, build_node_to_job_map,
        get_jobs, print_accounts,
    },
    nodes::get_nodes,
};
use std::collections::{BTreeMap, HashMap, HashSet};

const ALWAYS_SHOW: [&str; 2] = ["preempt", "gpupreempt"];

pub fn print_limits(name: &str, show_all: bool) {
    let all_partitions = get_partitions().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    // Naming the QOS keeps the reply to a fraction of its size: it otherwise carries every
    // QOS's per-user counters for every user on the cluster. Which partitions this user can
    // submit to is not known until the reply names their account, so ask about the QOS of
    // all of them rather than pay for a second round trip to narrow it further.
    let mut wanted: Vec<String> = all_partitions
        .iter()
        .filter_map(|partition| partition.effective_qos().map(str::to_string))
        .collect();
    wanted.sort();
    wanted.dedup();

    let assoc_mgr = load_assoc_mgr(vec![name.to_string()], Some(wanted)).unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(1);
    });

    // the counters are keyed by uid, and the account decides which partitions are on offer
    let user = assoc_mgr.users.get(name).unwrap_or_else(|| {
        eprintln!("Slurm has no user named \"{name}\"");
        std::process::exit(1);
    });
    let uid = user.uid;
    let user_acct = user.default_account.clone().unwrap_or_else(|| {
        eprintln!("\"{name}\" has no default account, so there is no center to report on");
        std::process::exit(1);
    });
    let usage = &assoc_mgr.qos;

    let partitions: Vec<_> = all_partitions
        .into_iter()
        .filter(|partition| partition.allows_account(&user_acct))
        .collect();

    // Partitions that share a QOS share one set of limits and one set of counters, so they
    // belong on one line: reported separately, each line would show the whole limit against
    // a fraction of the usage counted against it. Partitions with no QOS share nothing.
    let mut groups: BTreeMap<String, Group> = BTreeMap::new();

    for partition in &partitions {
        let qos = partition.effective_qos();
        let key = match qos {
            Some(qos) => format!("qos\u{1}{qos}"),
            None => format!("partition\u{1}{}", partition.name),
        };
        groups
            .entry(key)
            .or_insert_with(|| Group::new(qos.map(str::to_string)))
            .partitions
            .push(partition.name.clone());
    }

    let mut user_usage: Vec<AccountJobUsage> = Vec::new();
    let mut center_usage: Vec<AccountJobUsage> = Vec::new();

    for group in groups.values() {
        let label = group.label();
        // under -v the QOS the limits actually come from gets a column, since it is not
        // always named after the partitions drawing on it
        let qos_column = show_all.then(|| group.qos.clone().unwrap_or_else(|| "-".to_string()));

        // a partition with no QOS has no limits, and nothing counting against them
        let counted = group.qos.as_deref().and_then(|qos| usage.get(qos));
        let mine = counted.map(|qos| qos.user(uid)).unwrap_or_default();
        // the center table is about one account, not everyone sharing the QOS
        let ours = counted
            .map(|qos| qos.account(&user_acct))
            .unwrap_or_default();
        let limits: QosLimits = counted.map(|qos| qos.limits.clone()).unwrap_or_default();

        // A limit of zero cores or nodes admits no job at all, so the partition is closed
        // and saying so on every report is noise. Anything already running against it is
        // worth seeing, though, since the limit cannot be why it got there.
        let closed = [
            tres_limit(&limits.max_tres_per_user, "cpu"),
            tres_limit(&limits.max_tres_per_user, "node"),
            tres_limit(&limits.group_tres, "cpu"),
            tres_limit(&limits.group_tres, "node"),
        ]
        .contains(&Some(0));
        let idle = [
            mine.jobs,
            mine.cores(),
            mine.nodes(),
            mine.gpus(),
            ours.jobs,
            ours.cores(),
            ours.nodes(),
            ours.gpus(),
        ]
        .iter()
        .all(|&used| used == 0);

        if closed && idle && !show_all {
            continue;
        }

        user_usage.push(AccountJobUsage {
            account: label.clone(),
            qos: qos_column.clone(),
            cores: mine.cores(),
            nodes: mine.nodes(),
            gpus: mine.gpus(),
            jobs: mine.jobs,
            max_cores: tres_limit(&limits.max_tres_per_user, "cpu"),
            max_nodes: tres_limit(&limits.max_tres_per_user, "node"),
            max_gpus: tres_limit(&limits.max_tres_per_user, "gres/gpu"),
            max_jobs: limits.max_jobs_per_user,
        });
        center_usage.push(AccountJobUsage {
            account: label,
            qos: qos_column,
            cores: ours.cores(),
            nodes: ours.nodes(),
            gpus: ours.gpus(),
            jobs: ours.jobs,
            max_cores: tres_limit(&limits.group_tres, "cpu"),
            max_nodes: tres_limit(&limits.group_tres, "node"),
            max_gpus: tres_limit(&limits.group_tres, "gres/gpu"),
            // MaxJobsPU is a per-user limit, so the center has no counterpart to show
            max_jobs: None,
        });
    }

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

/// One TRES limit, narrowed to the width the report prints. Absent is no limit.
fn tres_limit(limits: &HashMap<String, u64>, tres: &str) -> Option<u32> {
    limits
        .get(tres)
        .map(|&limit| limit.try_into().unwrap_or(u32::MAX))
}

/// The partitions drawing on one QOS, which therefore share its limits and its counters
struct Group {
    partitions: Vec<String>,
    qos: Option<String>,
}

impl Group {
    fn new(qos: Option<String>) -> Self {
        Self {
            partitions: Vec::new(),
            qos,
        }
    }

    /// Every partition sharing the limits, so that one line can stand for all of them
    fn label(&self) -> String {
        let mut names = self.partitions.clone();
        names.sort();
        names.join(",")
    }
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
