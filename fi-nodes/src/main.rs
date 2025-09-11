pub mod report;
pub mod summary_report;
pub mod tree_report;

#[cfg(feature = "tui")]
pub mod tui;

#[cfg(feature = "tui")]
use crate::tui::app::tui_execute;

use clap::Parser;
use fi_slurm::filter::filter_nodes_by_feature;
use fi_slurm::jobs::{SlurmJobs, build_node_to_job_map, enrich_jobs_with_node_ids, get_jobs};
use fi_slurm::nodes::get_nodes;
use fi_slurm::nodes::{NodeState, SlurmNodes};
use fi_slurm::utils::{SlurmConfig, initialize_slurm};
use std::collections::{HashMap, HashSet};
use tree_report::{GpuFilter, build_tree_report, print_tree_report};

use chrono::{DateTime, Utc};
use std::time::Instant;

/// The main entry point for the `fi-nodes` utility
///
/// The function orchestrates the main pipeline:
/// 1. Load all node and job data from Slurm
/// 2. Create a cross-reference map to link nodes to the jobs running on them
/// 3. Aggregate all data into a structured report format
/// 4. Print the final, formatted report to the console
fn main() -> Result<(), String> {
    let start = Instant::now();

    let args = Args::parse();

    // entry point for the prometheus TUI utility
    #[cfg(feature = "tui")]
    {
        if args.term {
            let _ = tui_execute();
            return Ok(());
        }
    }

    if args.debug {
        println!("Started initializing Slurm: {:?}", start.elapsed());
    }

    // has no output, only passes a null pointer to Slurm directly in order to initialize
    // non-trivial functions of the Slurm API
    initialize_slurm();

    if args.debug {
        println!("Finished initializing Slurm: {:?}", start.elapsed());
    }

    // After initializing, we load the conf to get a handle that we can
    // manage for proper cleanup
    if args.debug {
        println!("Started loading Slurm config: {:?}", start.elapsed());
    }

    // We don't need to actually use this variable, but we store it anyway in order to
    // automatically invoke its Drop implementation when it goes out of scope at the end of main()
    let _slurm_config = SlurmConfig::load()?;
    if args.debug {
        println!("Finished loading Slurm config: {:?}", start.elapsed());
    }

    // Load Data
    if args.debug {
        println!("Starting to load Slurm data: {:?}", start.elapsed());
    }

    // Collect current node information from the cluster
    let mut nodes_collection = get_nodes()?;
    if args.debug {
        println!(
            "Finished loading node data from Slurm: {:?}",
            start.elapsed()
        );
    }

    // collect current job information from the cluster
    let mut jobs_collection = get_jobs()?;
    if args.debug {
        println!(
            "Finished loading job data from Slurm: {:?}",
            start.elapsed()
        );
    }

    // add the node ids instead of just node hostnames to the jobs collection
    // necessary in order for cross-referencing and creating the node to job mapping in the build
    // report functions
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

    // getting information on which nodes are preempted, to be used in the build report functions
    let preempted_nodes = if args.preempt {
        Some(preempt_node(
            &mut nodes_collection,
            &node_to_job_map,
            &jobs_collection,
        ))
    } else {
        None
    };

    // filtering nodes by feature
    let mut filtered_nodes = filter_nodes_by_feature(&nodes_collection, &args.feature, args.exact);
    if args.debug && !args.feature.is_empty() {
        println!("Finished filtering data: {:?}", start.elapsed());
    }

    // if all filtered nodes are GPU nodes, then automatically enable -g,
    // if the user did not specify -a
    let do_gpu_report = !args.all
        && (args.gpu
            || (!filtered_nodes.is_empty()
                && filtered_nodes.iter().all(|node| node.gpu_info.is_some())));

    // for filtering the final display
    let gpu_filter: GpuFilter = if args.all {
        GpuFilter::All
    } else if do_gpu_report {
        // not totally exclusive, but we want any use of --all/-a to override the
        // others
        GpuFilter::Gpu
    } else {
        // the default, we just show those which are not gpus
        GpuFilter::NotGpu
    };

    if args.debug {
        println!(
            "Successfully loaded {} nodes and {} jobs.",
            nodes_collection.nodes.len(),
            jobs_collection.jobs.len()
        );
        println!("Started building node to job map: {:?}", start.elapsed());
    }

    // entry point for the detailed report (replacement for nick carriero's featureInfo utility)
    if args.detailed {
        if args.debug {
            println!("Started building report: {:?}", start.elapsed());
        }

        //  Aggregate Data into Report
        let report = report::build_report(
            &filtered_nodes,
            &jobs_collection,
            &node_to_job_map,
            args.names,
            args.allocated,
            args.verbose,
        );
        if args.debug {
            println!("Aggregated data into {} state groups.", report.len());
            println!("Finished building detailed report: {:?}", start.elapsed());
        }

        // Print Report
        report::print_report(&report, args.no_color, args.names, args.allocated);
        if args.debug {
            println!("Finished printing report: {:?}", start.elapsed());
        }

        return Ok(());

    // the entry point for the summary report (DEPRECATED)
    } else if args.summary {
        // Aggregate data into summary report
        let summary_report = summary_report::build_summary_report(
            &filtered_nodes,
            &jobs_collection,
            &node_to_job_map,
        );
        if args.debug {
            println!(
                "Aggregated data into {} feature types.",
                summary_report.len()
            );
            println!("Finished building summary report: {:?}", start.elapsed());
        }

        summary_report::print_summary_report(&summary_report, args.no_color);

        return Ok(());
    } else {
        // filtering out nodes by gpuinfo if necessary
        // For example, we may have selected both GPU and CPU nodes with "icelake", but we
        // want to display one or the other set without -a
        match gpu_filter {
            GpuFilter::Gpu => {
                filtered_nodes.retain(|node| {
                    node.gpu_info.is_some() // if gpu info is some, that means there is a gpu
                });
            }
            GpuFilter::NotGpu => {
                filtered_nodes.retain(|node| {
                    node.gpu_info.is_none() // if gpu info is none, that means there is no gpu
                });
            }
            GpuFilter::All => {}
        }

        // Aggregate data into the tree report
        let tree_report = build_tree_report(
            &filtered_nodes,
            &jobs_collection,
            &node_to_job_map,
            &args.feature,
            args.verbose,
            args.names,
            preempted_nodes,
            args.preempt,
            do_gpu_report, // count GPUs instead of CPUs
        );
        print_tree_report(
            &tree_report,
            args.no_color,
            args.names,
            args.alphabetical,
            args.preempt,
            do_gpu_report, // display GPU column
        );

        if args.debug {
            println!("Finished building tree report: {:?}", start.elapsed());
        }
    }

    Ok(())
}

/// Newtype for the ids of preempted nodes
#[derive(Clone)]
pub struct PreemptNodes(Vec<usize>);

/// Function to crawl through the node to job map and change the status of a given node if the
/// job/s running on it are preempt.
///
/// If a preempt job is othe only one running on that node, we change its base state to Idle. If
/// a preempt job is one of several running on the node, we can change it from Allocated to Mixed,
/// assuming it was not already Mixed.
fn preempt_node(
    slurm_nodes: &mut SlurmNodes,
    node_to_job_map: &HashMap<usize, Vec<u32>>,
    slurm_jobs: &SlurmJobs,
) -> PreemptNodes {
    let now: DateTime<Utc> = Utc::now();

    let mut preemptable_jobs: HashSet<u32> = HashSet::new();

    // in order to figure out which nodes are preempt, we have to take the current UTC date time
    // and compare it to the preemptable_time feature in the job
    for job in slurm_jobs.jobs.values() {
        if job.preemptable_time <= now && job.preemptable_time != chrono::DateTime::UNIX_EPOCH {
            // we ensure that the time is not just 0, the start of the Unix Epoch
            preemptable_jobs.insert(job.job_id);
        }
    }

    let mut all_preempt = HashSet::new();
    let mut partially_preempt = HashSet::new();

    // we iterate through the nodes and the jobs on them, and collect them into the preempt lists
    for (node_id, jobs_on_node) in node_to_job_map.iter() {
        if jobs_on_node.is_empty() {
            continue;
        }

        let is_all_preempt = jobs_on_node
            .iter()
            .all(|job_id| preemptable_jobs.contains(job_id));

        if is_all_preempt {
            all_preempt.insert(*node_id);
        } else {
            let has_any_preempt = jobs_on_node
                .iter()
                .any(|job_id| preemptable_jobs.contains(job_id));

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
    // the other category

    let mut preempted_nodes: Vec<usize> = Vec::new();

    for node in slurm_nodes.nodes.iter_mut() {
        if all_preempt.contains(&node.id) {
            match &node.state {
                NodeState::Allocated | NodeState::Mixed => {
                    preempted_nodes.push(node.id);
                    node.state = NodeState::Idle
                }
                NodeState::Compound { base, flags } => match **base {
                    NodeState::Allocated | NodeState::Mixed => {
                        preempted_nodes.push(node.id);
                        node.state = NodeState::Compound {
                            base: Box::new(NodeState::Idle),
                            flags: flags.to_vec(),
                        }
                    }
                    _ => (),
                },
                _ => (),
            }
        } else if partially_preempt.contains(&node.id) {
            match &node.state {
                NodeState::Allocated => {
                    preempted_nodes.push(node.id);
                    node.state = NodeState::Mixed
                }
                NodeState::Compound { base, flags } => {
                    if **base == NodeState::Allocated {
                        preempted_nodes.push(node.id);
                        node.state = NodeState::Compound {
                            base: Box::new(NodeState::Mixed),
                            flags: flags.to_vec(),
                        }
                    }
                }
                _ => (),
            }
        }
    }

    PreemptNodes(preempted_nodes)
}

const HELP: &str = "Report the state of nodes in a Slurm cluster, grouped by feature (tree view, the default) or state (-d, detailed view). Only CPU nodes are shown by default in the tree view; use -g to show only GPU nodes or -a to see all. The graphical availability bars display absolute node counts.";

#[derive(Parser, Debug)]
#[command(
    version,
    after_help = HELP,
    after_long_help = format!("{}\n\n{}\n\n{}", HELP, fi_slurm::AUTHOR_HELP,
"See also https://grafana.flatironinstitute.org for cluster monitoring dashboards."),
)]
struct Args {
    #[arg(short, long)]
    #[arg(help = "Shows all nodes (CPU and GPU) in the tree view")]
    all: bool,

    #[arg(
        long,
        help = "Display allocated nodes instead of idle (use with --detailed)"
    )]
    allocated: bool,

    #[arg(long)]
    #[arg(
        help = "Sort the tree report at each level in alphabetical order instead of by total node count."
    )]
    alphabetical: bool,

    #[arg(long, hide = true)]
    #[arg(help = "Prints debug-level logging steps to terminal")]
    debug: bool,

    #[arg(short, long)]
    #[arg(help = "Prints the detailed report, showing nodes by Slurm state")]
    #[arg(
        long_help = "Shows a detailed, state-oriented view of cluster nodes. It divides the nodes into top-level state (Idle, Mixed, Allocated, Down, or Unknown) along with compound state flags like DRAIN, RES, MAINT when present, and provides a count of nodes and the availability/utilization of their cores and GPUs. Unlike the default tree report, the detailed report declares 'available' any CPU core or GPU which belongs to a node that is IDLE or which is unallocate on a MIXED node, regardless of compound state flags like DRAIN or MAINT. Nodes are displayed under their first feature. -g and -a have no effect in detailed mode. Use -v to show GPUs by type."
    )]
    detailed: bool,

    #[arg(short, long)]
    #[arg(help = "filter features only by exact match rather than substrings ")]
    #[arg(default_value_t = true, hide = true)]
    // TODO: non-exact not displaying right, but also probably not needed
    exact: bool,

    #[arg(
        help = "Node features to display, such as \"icelake\" or \"genoa\". Accepts multiple features.\nFor GPUs, use -g instead of \"gpu\"."
    )]
    feature: Vec<String>,

    #[arg(short, long)]
    #[arg(
        help = "Shows only gpu nodes in the tree view (default if all selected nodes have GPUs)"
    )]
    gpu: bool,

    #[arg(short, long)]
    #[arg(
        help = "Include preempt information in the output.\n\"123(-45)\" means 123 nodes are idle or preemptable, while 45 are preemptable."
    )]
    #[arg(
        long_help = "Reclassifies the base state of nodes according to the preemptability of the jobs running on them: an allocated node with some jobs which are preemptable will be reclassified as Mixed, while an Allocated or Mixed node where all jobs are preemptable will be reclassified as Idle."
    )]
    preempt: bool,

    #[arg(short, long)]
    #[arg(help = "Shows node names")]
    names: bool,

    #[arg(long)]
    #[arg(help = "Disable colors in output")]
    no_color: bool,

    #[cfg(feature = "tui")]
    #[arg(short, long)]
    #[arg(
        help = "[Experimental] Displays time-series cluster usage in an interactive Terminal User Interface (TUI)."
    )]
    #[arg(
        long_help = "[Experimental] The TUI shows time-series cluster usage data from Prometheus.The default display is the last 30 days in 1 day increments. The range and increment of data can be customized by selecting 'Custom Query' in setup. Note that the loading times from Prometheus are directly related to the number of requested increments. Requesting the last month's data in 1 minute increments will take a very, very long time."
    )]
    term: bool,

    #[arg(short, long)]
    #[arg(help = "In the tree report, shows hidden node features. In the detailed view, breaks out GPU types.")]
    verbose: bool,

    #[arg(short, long, hide = true)] // summary report is deprecated in favor of tree view
    #[arg(help = "Prints the top-level summary report for each feature type")]
    summary: bool,
}
