#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

pub mod bindings;
pub mod energy;
pub mod gres;
pub mod nodes;
pub mod jobs;

use std::{collections::HashMap, ffi::CStr};
use chrono::{Date, DateTime, NaiveDateTime, TimeZone, Utc};

use bindings::{acct_gather_energy_t, int_least8_t, node_info_msg_t, slurm_api_version, slurm_free_node_info_msg, slurm_load_node};
use energy::AcctGatherEnergy;
use gres::parse_gres_str;
use crate::jobs::{get_jobs, SlurmJobs};
use crate::nodes::{get_nodes, SlurmNodes};

// This line includes the bindings file that build.rs generates.
//include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

// fn main() {
//     println!("Calling into the Slurm C library...");
//
//     // FFI calls to C are unsafe. We must wrap them in an unsafe block.
//     unsafe {
//         // slurm_api_version() is a simple function that returns the API version as an integer.
//         // It's a perfect, safe test case.
//         let version = slurm_api_version();
//         println!("Successfully called slurm_api_version().");
//         println!("Slurm API Version: {}", version);
//     }
// }
// Import the public API functions and data types from our modules.
// The paths assume `nodes` and `jobs` are modules in your crate root.
// e.g., your lib.rs or main.rs has `pub mod nodes; pub mod jobs;`

/// The main entry point for the application.
fn main() -> Result<(), String> {
    println!("Loading data from Slurm...");

    // 1. Load all node and job information.
    // The `?` operator will handle any errors during the FFI calls or data conversion.
    let nodes_collection = get_nodes()?;
    let jobs_collection = get_jobs()?;

    println!(
        "Successfully loaded {} nodes and {} jobs.",
        nodes_collection.nodes.len(),
        jobs_collection.jobs.len()
    );

    // 2. Build the cross-reference map from node names to the jobs running on them.
    let node_to_jobs_map = build_node_to_job_map(&jobs_collection);

    println!(
        "Built a map cross-referencing {} nodes with active jobs.",
        node_to_jobs_map.len()
    );

    // --- Future steps would go here ---
    // For example, iterate through nodes, get their state, look up jobs
    // from the map, and aggregate the report data.

    // Example: Print jobs running on a specific node (if it exists)
    if let Some(jobs_on_node) = node_to_jobs_map.get("compute-01") {
        println!("Jobs running on compute-01: {:?}", jobs_on_node);
    }
    
    Ok(())
}

/// Builds a map where keys are node hostnames and values are a list of job IDs
/// running on that node.
fn build_node_to_job_map(jobs: &SlurmJobs) -> HashMap<String, Vec<u32>> {
    let mut node_map: HashMap<String, Vec<u32>> = HashMap::new();

    // Iterate through every job in the collection.
    for job in jobs.jobs.values() {
        // We only care about jobs that are actually running and have nodes assigned.
        if job.job_state != crate::jobs::JobState::Running || job.nodes.is_empty() {
            continue;
        }

        // Parse the Slurm hostlist string (e.g., "node-[01-03]") into a
        // list of individual hostnames.
        let expanded_nodes = parse_slurm_hostlist(&job.nodes);

        // For each individual node name, add the current job's ID to the map.
        for node_name in expanded_nodes {
            node_map
                .entry(node_name)
                .or_default() // If the node isn't in the map yet, insert an empty Vec.
                .push(job.job_id); // Push the job ID onto the Vec for that node.
        }
    }

    node_map
}

/// Parses a Slurm hostlist string into a `Vec<String>` of individual hostnames.
///
/// Example: "compute-[01-03,10]" -> ["compute-01", "compute-02", "compute-03", "compute-10"]
///
/// NOTE: This is a placeholder implementation. A robust implementation would need
/// to handle various complex cases (padding, multiple ranges, etc.).
fn parse_slurm_hostlist(hostlist_str: &str) -> Vec<String> {
    // TODO: This is a placeholder. A real implementation would require a proper
    // parser, potentially using regex or a small state machine, to handle the
    // full Slurm hostlist syntax.
    
    // For now, we'll just handle simple, comma-separated lists.
    if hostlist_str.contains('[') {
        eprintln!(
            "Warning: Hostlist range parsing is not yet implemented. Treating '{}' as a single node.",
            hostlist_str
        );
        return vec![hostlist_str.to_string()];
    }

    hostlist_str.split(',').map(String::from).collect()
}
