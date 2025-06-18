pub mod report;
pub mod summary_report;

use clap::Parser;
use std::collections::HashMap;
use fi_slurm::jobs::SlurmJobs;
use fi_slurm::{jobs, nodes, parser::parse_slurm_hostlist, utils::{SlurmConfig, initialize_slurm}};

/// The main entry point for the `fi-node`
///
/// The function orchestrates the main pipeline:
/// 1. Load all node and job data from Slurm
/// 2. Create a cross-reference map to link nodes to the jobs running on them
/// 3. Aggregate all data into a structured report format
/// 4. Print the final, formatted report to the console
fn main() -> Result<(), String> {

    let args = Args::parse();

    if args.help {
        print_help();
        return Ok(())
    }

    // This MUST be the very first Slurm function called 
    // We pass a null pointer to let Slurm find its config file automatically
    if args.debug { println!("Initializing Slurm library..."); } 
    // has no output, only passes a null pointer to Slurm directly in order to initialize
    // non-trivial functions of the Slurm API
    initialize_slurm();

    // After initializing, we load the conf to get a handle that we can
    // manage for proper cleanup
    if args.debug { println!("Loading Slurm configuration..."); }

    // We don't need to actually use this variable, but we store it anyway in order to
    // automatically invoke its Drop implementation when it goes out of scope at the end of main()
    let _slurm_config = SlurmConfig::load()?;
    if args.debug { println!("Slurm configuration loaded successfully."); }

    // Load Data 
    if args.debug { println!("Loading data from Slurm..."); }
    let nodes_collection = nodes::get_nodes()?;
    if args.debug { println!("Loaded node data"); }
    let jobs_collection = jobs::get_jobs()?;
    if args.debug { println!("Loaded job data"); }
    if args.debug {
        println!(
            "Successfully loaded {} nodes and {} jobs.",
            nodes_collection.nodes.len(),
            jobs_collection.jobs.len()
        );
    }

    // Build Cross-Reference Map 
    let node_to_job_map = build_node_to_job_map(&jobs_collection);
    if args.debug { 
        println!(
            "Built map cross-referencing {} nodes with active jobs.",
            node_to_job_map.len()
        ); 
    }
    if args.detailed {
        //  Aggregate Data into Report
        let report = report::build_report(&nodes_collection, &jobs_collection, &node_to_job_map);
        if args.debug { println!("Aggregated data into {} state groups.", report.len()); }

        // Print Report 
        if args.debug { println!("\n--- Slurm Node Feature Report ---"); }
        report::print_report(&report);
    } else {
        // Aggregate data into summary report
        let summary_report = summary_report::build_summary_report(&nodes_collection, &jobs_collection, &node_to_job_map);
        if args.debug { println!("Aggregated data into {} feature types.", summary_report.len()); }

        if args.debug { println!("\n--- Slurm Summary Report ---"); }
        summary_report::print_summary_report(&summary_report);
    }

    Ok(())
}

/// Builds a map where keys are node hostnames and values are a list of job IDs
/// running on that node
fn build_node_to_job_map(jobs: &SlurmJobs) -> HashMap<String, Vec<u32>> {
    let mut node_map: HashMap<String, Vec<u32>> = HashMap::new();

    // Iterate through every job in the collection
    for job in jobs.jobs.values() {
        // We only care about jobs that are actually running and have nodes assigned
        if job.job_state != crate::jobs::JobState::Running || job.nodes.is_empty() {
            continue;
        }

        // Expand the Slurm hostlist string
        // TODO: consider using Slurm's built-in parser instead
        let expanded_nodes = parse_slurm_hostlist(&job.nodes);

        // For each individual node name, add the current job's ID to the map
        for node_name in expanded_nodes {
            node_map
                .entry(node_name)
                .or_default() // If the node isn't in the map yet, insert an empty Vec
                .push(job.job_id); // Push the job ID onto the Vec for that node
        }
    }

    node_map
}

/// A clap argument to print the current documentation and arguments
fn print_help() {
    println!("\n
        Welcome to fi-node! This is a command-line utility for examining the available nodes in the cluster through the Slurm Job Scheduler. \n
        Usage: fi-node [OPTIONS] \n\n
        Options: \n
        -d, --detailed          Prints the detailed, state-by-state report of node availability \n
            --debug             Prints the step-by-step process of querying Slurm \n
        -h  --help              Prints the options and documentation for the fi-node command-line tool. You are here!
        "
    )
}


/// A command-line utility to report the state of a Slurm cluster,
/// showing a summary of nodes grouped by state and feature
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Enable debug-level logging to print the pipeline steps to terminal
    #[arg(long)]
    debug: bool,
    #[arg(short, long)]
    detailed: bool,
    #[arg(short, long)]
    help: bool,
}


