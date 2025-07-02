pub mod report;
pub mod summary_report;
pub mod tree_report;
pub mod tui;

use clap::Parser;
use std::collections::HashMap;
use fi_slurm::jobs::{SlurmJobs, enrich_jobs_with_node_ids};
use fi_slurm::{jobs, nodes, utils::{SlurmConfig, initialize_slurm}};
use fi_slurm::filter::{self, gather_all_features};
use fi_prometheus::{get_max_resource, get_usage_by, Cluster, Grouping, Resource};
use crate::tui::tui_execute;

use std::time::Instant;


/// The main entry point for the `fi-node`
///
/// The function orchestrates the main pipeline:
/// 1. Load all node and job data from Slurm
/// 2. Create a cross-reference map to link nodes to the jobs running on them
/// 3. Aggregate all data into a structured report format
/// 4. Print the final, formatted report to the console
fn main() -> Result<(), String> {

    let start = Instant::now();

    let args = Args::parse();

    if args.terminal {
        let _ = tui_execute();
        return Ok(())
    }

    if args.help {
        print_help();
        return Ok(())
    }

    if args.prometheus {
        println!("Time to start Prometheus calls: {:?}", start.elapsed());

        let acct_rusty = get_usage_by(Cluster::Rusty, Grouping::Account, Resource::Cpus, 7, "1d");
        let nodes_rusty = get_usage_by(Cluster::Rusty, Grouping::Nodes, Resource::Cpus, 7, "1d");
        let max_resource_rusty = get_max_resource(Cluster::Rusty, None, Resource::Cpus, None, None);

        let gpu_rusty = get_usage_by(Cluster::Rusty, Grouping::GpuType, Resource::Gpus, 7, "1d");
        let gpu_max_resource = get_max_resource(Cluster::Rusty, Some(Grouping::GpuType), Resource::Gpus, None, None);

        let acct_popeye = get_usage_by(Cluster::Popeye, Grouping::Account, Resource::Cpus, 7, "1d");
        let nodes_popeye = get_usage_by(Cluster::Popeye, Grouping::Nodes, Resource::Cpus, 7, "1d");
        let max_resource_popeye = get_max_resource(Cluster::Popeye, None, Resource::Cpus, None, None);

        println!("Time to finish Prometheus calls: {:?}", start.elapsed());

        println!("Rusty CPUs");
        println!("By Account: {:?}", acct_rusty);
        println!("By Node Type: {:?}", nodes_rusty);
        println!("Max CPU Use: {:?} \n", max_resource_rusty);

        println!("Rusty GPUs");

        println!("By GPU Type: {:?}", gpu_rusty);
        println!("Max GPU Use{:?} \n", gpu_max_resource);

        println!("Popeye CPUs");
        println!("By Account: {:?}", acct_popeye);
        println!("By Node Type: {:?}", nodes_popeye);
        println!("Max CPU Use: {:?}", max_resource_popeye);

        return Ok(())
    }

    if args.exact && args.feature.is_empty() {
        eprintln!("-e/--exact has no effect without the -f/--feature argument. Did you intend to filter by a feature?")
    }

    // This MUST be the very first Slurm function called 
    // We pass a null pointer to let Slurm find its config file automatically
    if args.debug { println!("Initializing Slurm library...");  
        println!("Started initializing Slurm: {:?}", start.elapsed()); 
    }
    // has no output, only passes a null pointer to Slurm directly in order to initialize
    // non-trivial functions of the Slurm API
    initialize_slurm();

    if args.debug { println!("Initializing Slurm library...");  
        println!("Finished initializing Slurm: {:?}", start.elapsed()); }

    // After initializing, we load the conf to get a handle that we can
    // manage for proper cleanup
    if args.debug { println!("Loading Slurm configuration..."); 
        println!("Finished loading Slurm config: {:?}", start.elapsed()); 
    }

    // We don't need to actually use this variable, but we store it anyway in order to
    // automatically invoke its Drop implementation when it goes out of scope at the end of main()
    let _slurm_config = SlurmConfig::load()?;
    if args.debug { println!("Slurm configuration loaded successfully."); 
        println!("Finished loading Slurm config: {:?}", start.elapsed()); 
    }

    // Load Data 
    if args.debug { println!("Loading data from Slurm..."); 
        println!("Starting to Load data from Slurm: {:?}", start.elapsed()); }

    let nodes_collection = nodes::get_nodes()?;
    if args.debug { println!("Loaded node data"); 
        println!("Finished loading node data from Slurm: {:?}", start.elapsed()); }

    let mut jobs_collection = jobs::get_jobs()?;
    if args.debug { println!("Loaded job data"); 
        println!("Finished loading job data from Slurm: {:?}", start.elapsed()); }

    enrich_jobs_with_node_ids(&mut jobs_collection, &nodes_collection.name_to_id);

    let filtered_nodes = filter::filter_nodes_by_feature(&nodes_collection, &args.feature, args.exact);
    if args.debug && !args.feature.is_empty() { println!("Filtered nodes by feature"); 
        println!("Finished filtering data: {:?}", start.elapsed()); }

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
    if args.debug { println!("Started building node to job map: {:?}", start.elapsed()); }
    let node_to_job_map = build_node_to_job_map(&jobs_collection);
    if args.debug { 
        println!(
            "Built map cross-referencing {} nodes with active jobs.",
            node_to_job_map.len()
        ); 
        println!("Finished building node to job map: {:?}", start.elapsed()); 
    }
    if args.detailed {
        if args.debug { println!("Started building report: {:?}", start.elapsed()); }
        //  Aggregate Data into Report
        let report = report::build_report(&filtered_nodes, &jobs_collection, &node_to_job_map);
        if args.debug { println!("Aggregated data into {} state groups.", report.len()); 
            println!("Finished building report: {:?}", start.elapsed()); 
        }

        // Print Report 
        report::print_report(&report, args.no_color);
        if args.debug { println!("Finished printing report: {:?}", start.elapsed()); }

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
        let tree_report = tree_report::build_tree_report(&filtered_nodes, &jobs_collection, &node_to_job_map, &args.feature, args.verbose);
        tree_report::print_tree_report(&tree_report, args.no_color);
    }

    Ok(())
}

/// Builds a map where keys are node hostnames and values are a list of job IDs
/// running on that node
fn build_node_to_job_map(slurm_jobs: &SlurmJobs) -> HashMap<usize, Vec<u32>> {
    let mut node_to_job_map: HashMap<usize, Vec<u32>> = HashMap::new();

    for job in slurm_jobs.jobs.values() {
        if job.job_state != crate::jobs::JobState::Running || job.node_ids.is_empty() {
            continue;
        }
        for &node_id in &job.node_ids {
            node_to_job_map.entry(node_id).or_default().push(job.job_id);
        }
    }
    node_to_job_map
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
    prometheus: bool,
    #[arg(long)]
    terminal: bool,
    #[arg(short, long)]
    verbose: bool,
    #[arg(short, long)]
    help: bool,
}


