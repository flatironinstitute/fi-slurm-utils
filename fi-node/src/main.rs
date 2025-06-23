pub mod report;
pub mod summary_report;
pub mod tree_report;

use clap::Parser;
use std::collections::HashMap;
use fi_slurm::jobs::SlurmJobs;
use fi_slurm::{jobs, nodes, parser::parse_slurm_hostlist, utils::{SlurmConfig, initialize_slurm}};
use fi_slurm::filter::{self, gather_all_features};

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

    if args.exact && args.feature.is_empty() {
        eprintln!("-e/--exact has no effect without the -f/--feature argument. Did you intend to filter by a feature?")
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

    let filtered_nodes = filter::filter_nodes_by_feature(&nodes_collection, &args.feature, args.exact);
    if args.debug && !args.feature.is_empty() { println!("Filtered nodes by feature")}

    // validating input
    if !args.feature.is_empty() && filtered_nodes.is_empty() {
        eprintln!("Warning, no nodes found matching the specifiad features");

        if args.exact {
            eprint!("\n Consider removing the `--exact` argument");
        }

        let _all_features = gather_all_features(&nodes_collection);
        // TODO: more detailed error suggestions, maybe strsim, maybe just print a list of node
        // names?
    }


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
        let report = report::build_report(&filtered_nodes, &jobs_collection, &node_to_job_map);
        if args.debug { println!("Aggregated data into {} state groups.", report.len()); }

        // Print Report 
        if args.debug { println!("\n--- Slurm Node Feature Report ---"); }
        report::print_report(&report, args.no_color);

        return Ok(())
    } else if args.summary {
        // Aggregate data into summary report
        let summary_report = summary_report::build_summary_report(&filtered_nodes, &jobs_collection, &node_to_job_map);
        if args.debug { println!("Aggregated data into {} feature types.", summary_report.len()); }

        if args.debug { println!("\n--- Slurm Summary Report ---"); }
        summary_report::print_summary_report(&summary_report, args.no_color);
        
        return Ok(())
    } else {
        // Aggregate data into the tree report 
        let tree_report = tree_report::build_tree_report(&filtered_nodes, &jobs_collection, &node_to_job_map, &args.feature);
        tree_report::print_tree_report(&tree_report, args.no_color);
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
        Usage: fi-node [OPTIONS] \n
        Options: 
        -d, --detailed          Prints the detailed, state-by-state report of node availability 
            --debug             Prints the step-by-step process of querying Slurm 
        -s  --summary           Prints the top-level summary report for each feature type
        -f  --feature           Select individual features to filter by. `--feature icelake` would only show information for icelake nodes.
                                For multiple features, separate them with spaces, such as `--feature genoa gpu skylake`
        -e  --exact             In combination with --feature, filter only by exact match rather than substrings
            --no-color          Disable colors in output
        -h  --help              Prints the options and documentation for the fi-node command-line tool. You are here!
        "
    )
}

/// A command-line utility to report the state of a Slurm cluster,
/// showing a summary of nodes grouped by state and feature
#[derive(Parser, Debug)]
#[command(version, about, long_about = None, disable_help_flag = true)]
struct Args {
    /// Enable debug-level logging to print the pipeline steps to terminal
    #[arg(long)]
    debug: bool,
    #[arg(short, long)]
    detailed: bool,
    #[arg(short, long)]
    summary: bool,
    #[arg(short, long, num_args = 1..)]
    feature: Vec<String>,
    #[arg(short, long)]
    exact: bool,
    #[arg(long)]
    no_color: bool,
    #[arg(short, long)]
    help: bool,
}


