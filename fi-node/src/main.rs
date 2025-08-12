pub mod report;
pub mod summary_report;
pub mod tree_report;
pub mod tui;

use clap::Parser;
use fi_slurm::nodes::{NodeState, SlurmNodes};
use std::collections::{HashMap, HashSet};
use fi_slurm::jobs::{enrich_jobs_with_node_ids, JobState, SlurmJobs, get_jobs};
use fi_slurm::utils::{SlurmConfig, initialize_slurm};
use fi_slurm::nodes::get_nodes;
use fi_slurm::filter::{gather_all_features, filter_nodes_by_feature};
use crate::tui::app::tui_execute;


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

    if args.leaderboard {
        println!(" \n We've moved! For the leaderboard, please check out the new fi-limits utility, currently at `~nposner/bin/fi-limits`!");
        return Ok(())
    }

    if args.term {
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

    let mut nodes_collection = get_nodes()?;
    if args.debug { println!("Finished loading node data from Slurm: {:?}", start.elapsed()); }

    let mut jobs_collection = get_jobs()?;
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

    let preempted_nodes = if args.preempt {
        Some(preempt_node(&mut nodes_collection, &node_to_job_map, &jobs_collection))
    } else { None };

    let filtered_nodes = filter_nodes_by_feature(&nodes_collection, &args.feature, args.exact);
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
        let report = report::build_report(&filtered_nodes, &jobs_collection, &node_to_job_map, args.names, args.allocated, args.verbose);
        if args.debug { println!("Aggregated data into {} state groups.", report.len()); 
            println!("Finished building detailed report: {:?}", start.elapsed()); 
        }

        // Print Report 
        report::print_report(&report, args.no_color, args.names, args.allocated);
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
        let tree_report = tree_report::build_tree_report(
            &filtered_nodes,
            &jobs_collection,
            &node_to_job_map,
            &args.feature,
            args.verbose,
            args.names,
            preempted_nodes,
            args.preempt,
        );
        tree_report::print_tree_report(
            &tree_report,
            args.no_color,
            args.names,
            args.alphabetical,
            args.preempt,
        );

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
        if job.job_state != JobState::Running || job.node_ids.is_empty() {
            continue;
        }
        for &node_id in &job.node_ids {
            node_to_job_map.entry(node_id).or_default().push(job.job_id);
        }
    }
    node_to_job_map
}

#[derive(Clone)]
pub struct PreemptNodes(Vec<usize>);

// function to crawl through the node to job map and change the status of a given node if the job/s
// running on it are preempt
fn preempt_node(
    slurm_nodes: &mut SlurmNodes, 
    node_to_job_map: &HashMap<usize, Vec<u32>>, 
    slurm_jobs: &SlurmJobs
) -> PreemptNodes {
    // iterate through the jobs, figure out which are preempt
    // grabbing their ids, cross-reference to the ids of the nodes they're running on. This can be
    // many to many
    // if a preempt job is the only one running on that node, change its base state to Idle
    // if a preempt job is one of several running on the node, we can change it from allocated to
    // Mixed, assuming it was not already mixed
    // 
    // iterate over jobs first, then check in the mappings which nodes might be changed
    // and then change those nodes accordingly

    // do we also want to return those job ids, or the ids of those nodes which were reclassified?
    // and maybe also how many cores? No, let's just stick to nodes for now
    let now: DateTime<Utc> = Utc::now();

    let mut preemptable_jobs: HashSet<u32> = HashSet::new();

    // in order to figure out which nodes are preempt, we have to take the current UTC date time
// and compare it to the preemptable_time feature in the job
    for job in slurm_jobs.jobs.values() {
        if job.preemptable_time <= now && job.preemptable_time != chrono::DateTime::UNIX_EPOCH { 
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

    let mut preempted_nodes: Vec<usize> = Vec::new();

    for node in slurm_nodes.nodes.iter_mut() {
        if all_preempt.contains(&node.id) {
            match &node.state {
                NodeState::Allocated | NodeState::Mixed => {
                    preempted_nodes.push(node.id);
                    node.state = NodeState::Idle
                },
                NodeState::Compound {base, flags} => {
                    match **base {
                        // NodeState::Allocated | NodeState::Mixed => {
                        NodeState::Allocated | NodeState::Mixed => {
                            preempted_nodes.push(node.id);
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
                    preempted_nodes.push(node.id);
                    node.state = NodeState::Mixed
                },
                NodeState::Compound {base, flags} => {
                    if **base == NodeState::Allocated {
                        preempted_nodes.push(node.id);
                        node.state = NodeState::Compound { base: Box::new(NodeState::Mixed), flags: flags.to_vec() }
                    }
                },
                _ => (),
            }
        }
    }

    PreemptNodes(preempted_nodes)
}

/// A command-line utility to report the state of a Slurm cluster,
/// showing a summary of nodes grouped by state and feature
#[derive(Parser, Debug)]
#[command(version, about, long_about = "This command-line and terminal application was built by Lehman Garrison, Nicolas Posner, Dylan Simon, and Alex Chavkin at the Scientific Computing Core of the Flatiron Institute. By default, it displays the availability of different node types as a tree diagram, as queried from the locally running instance of Slurm. For support, contact nposner@flatironinstitute.org")]
struct Args {
    #[arg(short, long)]
    #[arg(help = "Prints the detailed, state-by-state report of node availability")]
    #[arg(long_help = "Primarily meant for internal use by SCC members in order to get detailed views of the overall state of the cluster and its individual nodes. It divides the nodes into top-level state, Idle, Mixed, Allocated, Down, or Unknown, along with compound state flags like DRAIN, RES, MAINT when present, and provides a count of nodes and the availability/utilization of their cores and gpus. Unlike the default tree report, the detailed report declares 'available' any cpu core or gpu which belongs to a node that is IDLE or which is unallocate on a MIXED node, regardless of compound state flags like DRAIN or MAINT.")]
    detailed: bool,
    #[arg(short, long)]
    #[arg(help = "Displays historical cluster data in Terminal User Interface (TUI).")]
    #[arg(long_help = "The TUI queries the locally connected Slurm cluster using the 'http://prometheus/' URL and eagerly retrieves the data from the last 30 days in 1 day increments. The range and increment of data can be customized by selecting 'Custom Query' in setup. Note that the loading times from Prometheus are directly related to the number of requested increments, not the total time range. Requesting the last month's data in 1 minute increments will take a very, very long time.")]
    term: bool,
    #[arg(help = "Select individual features to filter by. `icelake` would only show information for icelake nodes. \n For multiple features, separate them with spaces, such as `genoa gpu skylake`")]
    feature: Vec<String>,
    #[arg(short, long)]
    #[arg(help = "In combination with --feature, filter only by exact match rather than substrings ")]
    exact: bool,
    #[arg(long, help = "Display allocated nodes instead of idle (use with --detailed)")]
    allocated: bool,
    #[arg(short, long)]
    #[arg(help = "Classifies preemptable jobs as idle")]
    #[arg(long_help = "Reclassifies the base state of nodes according to the preemptability of the jobs running on them: an allocated node with some jobs which are preemptable will be reclassified as Mixed, while an Allocated or Mixed node where all jobs are preemptable will be reclassified as Idle.")]
    preempt: bool,
    #[arg(short, long)]
    #[arg(help = "Shows all node names (not yet implemented in summary report)")]
    names: bool,
    #[arg(long)]
    #[arg(help = "Disable colors in output")]
    no_color: bool,
    #[arg(short, long)]
    #[arg(help = "Shows redundant nodes traits in tree report")]
    verbose: bool,
    #[arg(long)]
    #[arg(help = "Sort tree report hierarchy in alphabetical order instead of the default sorting by node count.")]
    alphabetical: bool,
    #[arg(long)]
    #[arg(help = "Prints debug-level logging steps to terminal")]
    debug: bool,
    #[arg(short, long)]
    #[arg(help = "Prints the top-level summary report for each feature type")]
    summary: bool,
    #[arg(long)]
    leaderboard: bool,
}



