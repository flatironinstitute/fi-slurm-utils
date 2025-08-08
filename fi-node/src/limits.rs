use std::collections::HashMap;
use fi_slurm::jobs::{JobState, get_jobs};

pub fn leaderboard(top_n: u32) {
    let mut map: HashMap<String, (u32, u32)> = HashMap::new();

    let jobs_collection = get_jobs()?;

    jobs_collection.jobs.iter().for_each(|(_, job)| {
        if job.job_state == JobState::Running {
            let usage = map.entry(job.user_name.clone()).or_insert((0, 0)); //(job.user_name, (job.num_nodes, job.num_cpus))

            usage.0 += job.num_nodes;
            usage.1 += job.num_cpus;
        }
    });

    let mut sorted_scores: Vec<(&String, &(u32, u32))> = map.iter().collect();

    sorted_scores.sort_by(|a, b| b.1.cmp(a.1));

    for (position, (user, score)) in sorted_scores.iter().enumerate().take(*top_n) {
        let rank = position + 1;
        let padding = if rank > 9 { "" } else {" "}; // just valid for the first 100
        let (initial, surname) = user.split_at_checked(1).unwrap_or(("Dr", "Evil"));
        println!("{}. {} {}. {} is using {} nodes and {} cores", rank, padding, initial.to_uppercase(), surname.to_uppercase(), score.0, score.1);
    }
}
