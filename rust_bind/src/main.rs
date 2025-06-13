#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

pub mod bindings;
pub mod energy;
pub mod gres;
pub mod nodes;
pub mod jobs;
pub mod parser;
pub mod report;

use std::collections::HashMap;

use crate::jobs::{get_jobs, SlurmJobs};
use crate::nodes::get_nodes;

// This line includes the bindings file that build.rs generates.
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

 //fn main() {
 //    println!("Calling into the Slurm C library...");
 //
 //    // FFI calls to C are unsafe. We must wrap them in an unsafe block.
 //    unsafe {
 //        // slurm_api_version() is a simple function that returns the API version as an integer.
 //        // It's a perfect, safe test case.
 //        let version = slurm_api_version();
 //        println!("Successfully called slurm_api_version().");
 //        println!("Slurm API Version: {}", version);
 //    }
 //}
// Import the public API functions and data types from our modules.
// The paths assume `nodes` and `jobs` are modules in your crate root.
// e.g., your lib.rs or main.rs has `pub mod nodes; pub mod jobs;`

/// The main entry point for the `feature-report-rust` application.
///
/// The function orchestrates the main pipeline:
/// 1. Load all node and job data from Slurm.
/// 2. Create a cross-reference map to link nodes to the jobs running on them.
/// 3. Aggregate all data into a structured report format.
/// 4. Print the final, formatted report to the console.
fn main() -> Result<(), String> {

    // --- 0. Global Slurm Initialization ---
    // This MUST be the very first Slurm function called. It initializes
    // the library, setting up logging and loading essential plugins like crypto.
    // We pass a null pointer to let Slurm find its config file automatically.
    println!("Initializing Slurm library...");
    unsafe {
        bindings::slurm_init(std::ptr::null());
    }

    // --- 1. Load Configuration into RAII Guard ---
    // After initializing, we load the conf to get a handle that we can
    // manage for proper cleanup.
    println!("Loading Slurm configuration...");
    let _slurm_config = SlurmConfig::load()?;
    println!("Slurm configuration loaded successfully.");

    // --- 2. Load Data ---
    println!("Loading data from Slurm...");
    let nodes_collection = nodes::get_nodes()?;
    println!("Loaded node data");
    let jobs_collection = jobs::get_jobs()?;
    println!("Loaded job data");
    println!(
        "Successfully loaded {} nodes and {} jobs.",
        nodes_collection.nodes.len(),
        jobs_collection.jobs.len()
    );

    // --- 3. Build Cross-Reference Map ---
    let node_to_job_map = build_node_to_job_map(&jobs_collection);
    println!(
        "Built map cross-referencing {} nodes with active jobs.",
        node_to_job_map.len()
    );

    // --- 4. Aggregate Data into Report Format ---
    let report = report::build_report(&nodes_collection, &jobs_collection, &node_to_job_map);
    println!("Aggregated data into {} state groups.", report.len());

    // --- 4. Print Final Report ---
    println!("\n--- Slurm Node Feature Report ---");
    report::print_report(&report);

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

        // Use our robust parser to expand the Slurm hostlist string.
        let expanded_nodes = parser::parse_slurm_hostlist(&job.nodes);

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

// This struct ensures that the Slurm configuration is automatically
// loaded on creation and freed when it goes out of scope.
struct SlurmConfig {
    // We don't need to access the pointer, but we need to hold it
    // so we can free it in the Drop implementation.
    _ptr: *mut bindings::slurm_conf_t,
}

impl SlurmConfig {
    /// Loads the Slurm configuration and returns a guard object.
    /// The configuration will be freed when the guard is dropped.
    pub fn load() -> Result<Self, String> {
        let mut conf_ptr: *mut bindings::slurm_conf_t = std::ptr::null_mut();
        unsafe {
            if bindings::slurm_load_ctl_conf(0, &mut conf_ptr) != 0 {
                return Err(
                    "Failed to load slurm.conf. Check logs or SLURM_CONF env variable."
                        .to_string(),
                );
            }
        }
        if conf_ptr.is_null() {
            // This is a defensive check; slurm_load_ctl_conf should not return 0
            // and a null pointer, but we check just in case.
            return Err("slurm_load_ctl_conf succeeded but returned a null pointer.".to_string());
        }
        Ok(SlurmConfig { _ptr: conf_ptr })
    }
}

impl Drop for SlurmConfig {
    fn drop(&mut self) {
        // This is guaranteed to be called when the SlurmConfig instance
        // goes out of scope, preventing memory leaks.
        unsafe {
            bindings::slurm_free_ctl_conf(self._ptr);
        }
        println!("\nCleaned up Slurm configuration.");
    }
}

//fn parse_slurm_hostlist(hostlist_str: &str) -> Vec<String> {
//    // TODO: This is a placeholder. A real implementation would require a proper
//    // parser, potentially using regex or a small state machine, to handle the
//    // full Slurm hostlist syntax.
//
//    // For now, we'll just handle simple, comma-separated lists.
//    if hostlist_str.contains('[') {
//        eprintln!(
//            "Warning: Hostlist range parsing is not yet implemented. Treating '{}' as a single node.",
//            hostlist_str
//        );
//        return vec![hostlist_str.to_string()];
//    }
//
//    hostlist_str.split(',').map(String::from).collect()
//}



///// The main entry point for the application.
//fn main() -> Result<(), String> {
//    println!("Loading data from Slurm...");
//
//    // 1. Load all node and job information.
//    // The `?` operator will handle any errors during the FFI calls or data conversion.
//    let nodes_collection = get_nodes()?;
//    let jobs_collection = get_jobs()?;
//
//    println!(
//        "Successfully loaded {} nodes and {} jobs.",
//        nodes_collection.nodes.len(),
//        jobs_collection.jobs.len()
//    );
//
//    // 2. Build the cross-reference map from node names to the jobs running on them.
//    let node_to_jobs_map = build_node_to_job_map(&jobs_collection);
//
//    println!(
//        "Built a map cross-referencing {} nodes with active jobs.",
//        node_to_jobs_map.len()
//    );
//
//    // --- Future steps would go here ---
//    // For example, iterate through nodes, get their state, look up jobs
//    // from the map, and aggregate the report data.
//
//    // Example: Print jobs running on a specific node (if it exists)
//    if let Some(jobs_on_node) = node_to_jobs_map.get("compute-01") {
//        println!("Jobs running on compute-01: {:?}", jobs_on_node);
//    }
//
//    Ok(())

