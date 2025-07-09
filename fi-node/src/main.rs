pub mod report;
pub mod summary_report;
pub mod tree_report;
pub mod tui;

use clap::Parser;
use fi_slurm::nodes::{NodeState, SlurmNodes};
use std::collections::{HashMap, HashSet};
use fi_slurm::jobs::{SlurmJobs, enrich_jobs_with_node_ids};
use fi_slurm::{jobs, nodes, utils::{SlurmConfig, initialize_slurm}};
use fi_slurm::filter::{self, gather_all_features};
use crate::tui::tui_execute;

use std::time::Instant;
use chrono::{DateTime, Utc};


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

    if args.exact && args.feature.is_empty() {
        eprintln!("-e/--exact has no effect without the -f/--feature argument. Did you intend to filter by a feature?")
    }

    if args.debug { println!("Started initializing Slurm: {:?}", start.elapsed()); }
    
    // has no output, only passes a null pointer to Slurm directly in order to initialize
    // non-trivial functions of the Slurm API
    initialize_slurm();

    if args.debug { println!("Finished initializing Slurm: {:?}", start.elapsed()); }

    // After initializing, we load the conf to get a handle that we can
    // manage for proper cleanup
    if args.debug { println!("Started loading Slurm config: {:?}", start.elapsed()); }

    // We don't need to actually use this variable, but we store it anyway in order to
    // automatically invoke its Drop implementation when it goes out of scope at the end of main()
    let _slurm_config = SlurmConfig::load()?;
    if args.debug { println!("Finished loading Slurm config: {:?}", start.elapsed()); }

    // Load Data 
    if args.debug { println!("Starting to load Slurm data: {:?}", start.elapsed()); }

    let mut nodes_collection = nodes::get_nodes()?;
    if args.debug { println!("Finished loading node data from Slurm: {:?}", start.elapsed()); }

    let mut jobs_collection = jobs::get_jobs()?;
    if args.debug { println!("Finished loading job data from Slurm: {:?}", start.elapsed()); }

    enrich_jobs_with_node_ids(&mut jobs_collection, &nodes_collection.name_to_id);

    // Build Cross-Reference Map 
    let node_to_job_map = build_node_to_job_map(&jobs_collection);
    if args.debug { 
        println!(
            "Built map cross-referencing {} nodes with active jobs.",
            node_to_job_map.len()
        ); 
        println!("Finished building node to job map: {:?}", start.elapsed()); 
    }

    if args.preempt {
        preempt_node(&mut nodes_collection, &node_to_job_map, &jobs_collection);
    }

    let filtered_nodes = filter::filter_nodes_by_feature(&nodes_collection, &args.feature, args.exact);
    if args.debug && !args.feature.is_empty() { println!("Finished filtering data: {:?}", start.elapsed()); }

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
        println!("Started building node to job map: {:?}", start.elapsed()); 
    }



    if args.detailed {
        if args.debug { println!("Started building report: {:?}", start.elapsed()); }
        //  Aggregate Data into Report
        let report = report::build_report(&filtered_nodes, &jobs_collection, &node_to_job_map, args.names);
        if args.debug { println!("Aggregated data into {} state groups.", report.len()); 
            println!("Finished building detailed report: {:?}", start.elapsed()); 
        }

        // Print Report 
        report::print_report(&report, args.no_color, args.names);
        if args.debug { println!("Finished printing report: {:?}", start.elapsed()); }

        return Ok(())
    } else if args.summary {
        // Aggregate data into summary report
        let summary_report = summary_report::build_summary_report(&filtered_nodes, &jobs_collection, &node_to_job_map);
        if args.debug { println!("Aggregated data into {} feature types.", summary_report.len()); 
            println!("Finished building summary report: {:?}", start.elapsed()); 
        }

        summary_report::print_summary_report(&summary_report, args.no_color);
        
        return Ok(())
    } else {
        // Aggregate data into the tree report 
        let tree_report = tree_report::build_tree_report(&filtered_nodes, &jobs_collection, &node_to_job_map, &args.feature, args.verbose, args.names);
        tree_report::print_tree_report(&tree_report, args.no_color, args.names);

        if args.debug { println!("Finished building tree report: {:?}", start.elapsed()); }
    }

    Ok(())
}

// taking into account preempt jobs that we may want to classify as idle for some purposes
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

/// A command-line utility to report the state of a Slurm cluster,
/// showing a summary of nodes grouped by state and feature
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Enable debug-level logging to print the pipeline steps to terminal
    #[arg(long)]
    #[arg(help = "Prints the step-by-step process of querying Slurm")]
    debug: bool,
    #[arg(short, long)]
    #[arg(help = "Prints the detailed, state-by-state report of node availability")]
    detailed: bool,
    #[arg(short, long)]
    #[arg(help = "Prints the top-level summary report for each feature type")]
    summary: bool,
    #[arg(help = "Select individual features to filter by. `icelake` would only show information for icelake nodes. \n For multiple features, separate them with spaces, such as `genoa gpu skylake`")]
    feature: Vec<String>,
    #[arg(short, long)]
    #[arg(help = "In combination with --feature, filter only by exact match rather than substrings ")]
    exact: bool,
    #[arg(long)]
    #[arg(help = "Disable colors in output")]
    no_color: bool,
    #[arg(long)]
    #[arg(help = "Activates TUI (in development)")]
    terminal: bool,
    #[arg(short, long)]
    #[arg(help = "Shows redundant nodes traits in tree report")]
    verbose: bool,
    #[arg(short, long)]
    #[arg(help = "Shows all node names (not yet implemented in summary report)")]
    names: bool,
    #[arg(short, long)]
    #[arg(help = "Classifies preemptable jobs as idle")]
    preempt: bool,
}



// function to crawl through the node to job map and change the status of a given node if the job/s
// running on it are preempt
fn preempt_node(
    slurm_nodes: &mut SlurmNodes, 
    node_to_job_map: &HashMap<usize, Vec<u32>>, 
    slurm_jobs: &SlurmJobs
) {
    // iterate through the jobs, figure out which are preempt
    // grabbing their ids, cross-reference to the ids of the nodes they're running on. This can be
    // many to many
    // if a preempt job is the only one running on that node, change its base state to Idle
    // if a preempt job is one of several running on the node, we can change it from allocated to
    // Mixed, assuming it was not already mixed
    // 
    // iterate over jobs first, then check in the mappings which nodes might be changed
    // and then change those nodes accordingly

    let now: DateTime<Utc> = Utc::now();

    let mut preemptable_jobs: HashSet<u32> = HashSet::new();

    // in order to figure out which nodes are preempt, we have to take the current UTC date time
// and compare it to the preemptable_time feature in the job
    for job in slurm_jobs.jobs.values() {
        
        if job.preemptable_time <= now {
            preemptable_jobs.insert(job.job_id);
        }
    }

    let mut all_preempt = HashSet::new();
    let mut partially_preempt = HashSet::new();

    for (node_id, jobs_on_node) in node_to_job_map.iter() {
        if jobs_on_node.is_empty() {
            continue;
        }

        let is_all_preempt = jobs_on_node.iter().all(|job_id| preemptable_jobs.contains(job_id));

        if is_all_preempt {
            all_preempt.insert(*node_id);
        } else {
            let has_any_preempt = jobs_on_node.iter().any(|job_id| preemptable_jobs.contains(job_id));

            if has_any_preempt {
                partially_preempt.insert(*node_id);
            }
        }
    }

    // having both lists, now we go through SlurmNodes.nodes, check ids, and convert the base
    // node_state, taking into account compound states as well
    //
    // for nodes in the all_preempt vector, we want to turn allocated and mixed nodes to idle, and 
    // compound allocated/mixed to idle
    //
    // for nodes in the partially_preempt list, we want to turn allocated into mixed 
    // we leave mixed be, because if the jobs running on it were all preempt, the node would be in
    // the othe category

    for node in slurm_nodes.nodes.iter_mut() {
        if all_preempt.contains(&node.id) {
            match &node.state {
                // NodeState::Allocated | NodeState::Mixed => {
                NodeState::Allocated => {
                    node.state = NodeState::Idle
                },
                NodeState::Compound {base, flags} => {
                    match **base {
                        // NodeState::Allocated | NodeState::Mixed => {
                        NodeState::Allocated => {
                            node.state = NodeState::Compound { base: Box::new(NodeState::Idle), flags: flags.to_vec() }
                        },
                        _ => (),
                    }
                },
                _ => (),
            }
        } else if partially_preempt.contains(&node.id) {
            match &node.state {
                NodeState::Allocated => {
                    node.state = NodeState::Mixed
                },
                NodeState::Compound {base, flags} => {
                    if **base == NodeState::Allocated {
                        node.state = NodeState::Compound { base: Box::new(NodeState::Mixed), flags: flags.to_vec() }
                    }
                    //match **base {
                    //    NodeState::Allocated => {
                    //        node.state = NodeState::Compound { base: Box::new(NodeState::Mixed), flags: flags.to_vec() }
                    //    }
                    //    _ => (),
                    //}
                },
                _ => (),
            }
        }
    }
}
