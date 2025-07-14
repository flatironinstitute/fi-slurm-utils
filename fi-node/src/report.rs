use crate::nodes::{NodeState, Node};
use crate::jobs::SlurmJobs;
use fi_slurm::utils::count_blocks;
use std::collections::HashMap;
use colored::*;

/// Represents the aggregated statistics for a single line in the final report
///
/// This struct is used for both the main summary lines (e.g., "IDLE 197...")
/// and the indented subgroup lines (e.g., "  genoa  8...")
///
/// `#[derive(Default)]` allows us to easily create a new, zeroed-out instance
#[derive(Default, Debug, Clone)]
pub struct ReportLine {
    pub node_count: u32,
    pub total_cpus: u32,
    pub alloc_cpus: u32,
    pub total_gpus: u64,
    pub alloc_gpus: u64,
    pub node_names: Vec<String>,
}


/// Represents a top-level group in the report, categorized by a `NodeState`
///
/// For example, this would hold all the data for the "IDLE" or "MIXED" sections
#[derive(Default, Debug, Clone)]
pub struct ReportGroup {
    /// The aggregated statistics for the main summary line of this group
    pub summary: ReportLine,
    /// A map holding the statistics for each subgroup, keyed by feature or GRES name
    /// For example: `{"genoa": ReportLine, "h100": ReportLine}`
    pub subgroups: HashMap<String, ReportLine>,
}

/// The final data structure that holds the entire report, organized by `NodeState`
///
/// This will be a map from a `NodeState` enum to its corresponding `ReportGroup`
/// Using a HashMap makes it easy to add stats to the correct group as we
/// iterate through all the nodes
pub type ReportData = HashMap<NodeState, ReportGroup>;


/// Builds the final report by aggregating data from all nodes
///
/// This function iterates through every node, categorizes it by its state,
/// and calculates summary statistics for each state and its subgroups
///
/// # Arguments
///
/// * `nodes` - A reference to the fully loaded `SlurmNodes` collection
/// * `jobs` - A reference to the fully loaded `SlurmJobs` collection
/// * `node_to_job_map` - A map from node names to the jobs running on them
///
/// # Returns
///
/// A `ReportData` HashMap containing all the aggregated information for display
pub fn build_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<usize, Vec<u32>>,
    show_node_names: bool,
) -> ReportData {
    let mut report_data = ReportData::new();

    // This creates a new Vec<u32> where each element corresponds to a node in the input `n` slice
    let alloc_cpus_per_node: Vec<u32> = nodes
        .iter()
        .map(|&node| {
            // For each node, look up its job IDs, returning an option
            node_to_job_map
                .get(&node.id)
                // If the Option is Some(job_ids), we map over it to perform the calculation
                .map(|job_ids| {
                    job_ids
                        .iter()
                        // For each job_id, look up the job details. filter_map unwraps the Some results
                        .filter_map(|job_id| jobs.jobs.get(job_id))
                        // For each valid job, calculate the CPUs allocated to this specific node
                        .map(|job| {
                            if job.num_nodes > 0 {
                                job.num_cpus / job.num_nodes
                            } else {
                                job.num_cpus // Should not happen, but handles malformed job data
                            }
                        })
                        .sum() // Sum the CPUs for all jobs on this node
                })
                // If the initial .get() returned None (no jobs on the node), default to 0
                .unwrap_or(0)
        })
        .collect(); // Collect all the results into our vector

    for (node, &alloc_cpus_for_node) in nodes.iter().zip(alloc_cpus_per_node.iter()) {
        // Slurm does not mark nodes as mixed by default, so we have to do it
        let derived_state = if alloc_cpus_for_node > 0 && alloc_cpus_for_node < node.cpus as u32 {
            NodeState::Mixed
        } else {
            // Otherwise, we trust the state reported by Slurm
            node.state.clone()
        };

        // Get the report group for the node's derived state
        let group = report_data.entry(derived_state).or_default();

        // Update the Main Summary Line for the Group 
        group.summary.node_count += 1;
        group.summary.total_cpus += node.cpus as u32;
        group.summary.alloc_cpus += alloc_cpus_for_node;
        if show_node_names { group.summary.node_names.push(node.name.clone()); }

        // Check if the node has GPU info
        if let Some(gpu) = &node.gpu_info {
            // The subgroup is identified by the specific GPU name
            group.summary.total_gpus += gpu.total_gpus;
            group.summary.alloc_gpus += gpu.allocated_gpus;

            let subgroup_line = group.subgroups.entry(gpu.name.clone()).or_default();
            
            // Add ALL stats for this node to this one subgroup
            subgroup_line.node_count += 1;
            subgroup_line.total_cpus += node.cpus as u32;
            subgroup_line.alloc_cpus += alloc_cpus_for_node;
            subgroup_line.total_gpus += gpu.total_gpus;
            subgroup_line.alloc_gpus += gpu.allocated_gpus;
            if show_node_names { subgroup_line.node_names.push(node.name.clone()); }
        }

        // This is a CPU-only Node
        // Fall back to using the first feature as its identity
        if let Some(feature) = node.features.first() {
            let subgroup_line = group.subgroups.entry(feature.clone()).or_default();
            
            // Add its stats. GPU counts will naturally be zero
            subgroup_line.node_count += 1;
            subgroup_line.total_cpus += node.cpus as u32;
            subgroup_line.alloc_cpus += alloc_cpus_for_node;
            if show_node_names { subgroup_line.node_names.push(node.name.clone()); }
        }
    };
    report_data
}

/// Formats and prints the aggregated report data to the console
pub fn print_report(report_data: &ReportData, no_color: bool, show_node_names: bool,) {
    // Pre-calculate all column widths
    let mut max_state_width = "STATE".len();
    let mut max_alloc_cpu_width = 0;
    let mut max_total_cpu_width = 0;
    let mut max_alloc_gpu_width = 0;
    let mut max_total_gpu_width = 0;

    let mut total_line_for_width = ReportLine::default();
    for group in report_data.values() {
        total_line_for_width.alloc_cpus += group.summary.alloc_cpus;
        total_line_for_width.total_cpus += group.summary.total_cpus;
    }
    max_alloc_cpu_width = max_alloc_cpu_width.max(total_line_for_width.alloc_cpus.to_string().len());
    max_total_cpu_width = max_total_cpu_width.max(total_line_for_width.total_cpus.to_string().len());

    for (state, group) in report_data.iter() {
        max_state_width = max_state_width.max(state.to_string().len());
        max_alloc_cpu_width = max_alloc_cpu_width.max(group.summary.alloc_cpus.to_string().len());
        max_total_cpu_width = max_total_cpu_width.max(group.summary.total_cpus.to_string().len());
        max_alloc_gpu_width = max_alloc_gpu_width.max(group.summary.alloc_gpus.to_string().len());
        max_total_gpu_width = max_total_gpu_width.max(group.summary.total_gpus.to_string().len());

        for subgroup_name in group.subgroups.keys() {
            max_state_width = max_state_width.max(subgroup_name.len() + 2);
        }
        for subgroup_line in group.subgroups.values() {
            max_alloc_cpu_width = max_alloc_cpu_width.max(subgroup_line.alloc_cpus.to_string().len());
            max_total_cpu_width = max_total_cpu_width.max(subgroup_line.total_cpus.to_string().len());
            max_alloc_gpu_width = max_alloc_gpu_width.max(subgroup_line.alloc_gpus.to_string().len());
            max_total_gpu_width = max_total_gpu_width.max(subgroup_line.total_gpus.to_string().len());
        }
    }
    max_state_width += 2;

    // Sort states for consistent output
    let state_order: HashMap<NodeState, usize> = [
        (NodeState::Idle, 0),
        (NodeState::Mixed, 1),
        (NodeState::Allocated, 2),
        (NodeState::Error, 3),
        (NodeState::Down, 4),
    ]
    .iter()
    .cloned()
    .collect();

    let mut sorted_states: Vec<&NodeState> = report_data.keys().collect();
    sorted_states.sort_by_key(|a| {
        if let NodeState::Compound { base, .. } = a {
            state_order.get(base).unwrap_or(&99)
        } else {
            state_order.get(a).unwrap_or(&99)
        }
    });

    // Print Headers
    let cpu_header = "CPU (A/T)";
    let gpu_header = "GPU (A/T)";
    let cpu_col_width = max_alloc_cpu_width + max_total_cpu_width + 2; // +3 for " / "
    let gpu_col_width = max_alloc_gpu_width + max_total_gpu_width + 3;
    
    println!(
        "{:<width$} {:>5} {:>cpu_w$}  {:>gpu_w$}",
        "STATE".bold(), "COUNT".bold(), cpu_header.bold(), gpu_header.bold(),
        width = max_state_width,
        cpu_w = cpu_col_width,
        gpu_w = gpu_col_width,
    );
    println!("{}", "-".repeat(max_state_width + cpu_col_width + gpu_col_width + 9));

    // Generate and Print Report Body
    let total_line = generate_report_body(report_data, sorted_states, max_state_width, (max_alloc_cpu_width, max_total_cpu_width, max_alloc_gpu_width, max_total_gpu_width), no_color, show_node_names);

    // Print Totals and Utilization Bars
    println!("{}", "-".repeat(max_state_width + cpu_col_width + gpu_col_width + 9));
    let total_cpu_str = format_stat_column(total_line.alloc_cpus as u64, total_line.total_cpus as u64, max_alloc_cpu_width, max_total_cpu_width);
    let total_gpu_str = format_stat_column(total_line.alloc_gpus, total_line.total_gpus, max_alloc_gpu_width, max_total_gpu_width);
    let total_padding = " ".repeat(max_state_width - "TOTAL".len());
    print!("{}{}", "TOTAL".bold(), total_padding);
    println!("{:>5}  {}  {}", total_line.node_count, total_cpu_str, total_gpu_str);

    let utilized_nodes = get_node_utilization(report_data);
    if total_line.node_count > 0 {
        let utilization_percent = (utilized_nodes / total_line.node_count as f64) * 100.0;
        print_utilization(utilization_percent, 50, BarColor::Green, "Node", no_color);
    }
    if total_line.total_cpus > 0 {
        let utilization_percent = (total_line.alloc_cpus as f64 / total_line.total_cpus as f64) * 100.0;
        print_utilization(utilization_percent, 50, BarColor::Cyan, "CPU", no_color);
    }
    if total_line.total_gpus > 0 {
        let utilization_percent = (total_line.alloc_gpus as f64 / total_line.total_gpus as f64) * 100.0;
        print_utilization(utilization_percent, 50, BarColor::Red, "GPU", no_color);
    }
}

/// Formats a statistics column (e.g., "5 / 100") with digit alignment
fn format_stat_column(
    alloc: u64,
    total: u64,
    max_alloc_width: usize,
    max_total_width: usize,
) -> String {
    format!(
        "{:>alloc_w$}/{:>total_w$}",
        alloc,
        total,
        alloc_w = max_alloc_width,
        total_w = max_total_width -1
    )
}

fn get_node_utilization(report_data: &HashMap<NodeState, ReportGroup>) -> f64 {
    let utilized_nodes = report_data.iter().fold(0, |account, (state, group)| {
        let is_utilized = match state {
            NodeState::Allocated | NodeState::Mixed => true,
            NodeState::Compound{ base, ..} => matches!(**base, NodeState::Allocated | NodeState::Mixed),
            _ => false,
        };
        if is_utilized { account + group.summary.node_count } else { account }
    });
    utilized_nodes as f64
}

#[derive(Clone)]
enum BarColor {
    Red,
    Green,
    Cyan
}

impl BarColor {
    pub fn apply_color(&self, text: &str) -> ColoredString {
        match self {
            BarColor::Cyan => text.cyan(),
            BarColor::Red => text.red(),
            BarColor::Green => text.green()
        }
    }
}

fn print_utilization(utilization_percent: f64, bar_width: usize, bar_color: BarColor, name: &str, no_color: bool) {
    // Call count_blocks to get the components of the bar
    let (full, empty, partial_opt) = count_blocks(bar_width, utilization_percent / 100.0);

    // Create the string for the full blocks
    let full_bar = "█".repeat(full);

    // Get the partial block character, or an empty string if there isn't one
    let partial_bar = partial_opt.unwrap_or_default();

    // Create the string for the empty space. Using a simple space is often cleaner
    let empty_bar = " ".repeat(empty);

    // Apply color to the filled parts of the bar
    let colored_full = if no_color { full_bar.white() } else { bar_color.apply_color(&full_bar) };
    let colored_partial = if no_color { partial_bar.white() } else { bar_color.apply_color(&partial_bar) };

    // Print the assembled bar
    println!(
        "Overall {} Utilization: \n [{}{}{}] {:.1}%",
        name,
        colored_full,
        colored_partial,
        empty_bar, // The empty part is not colored.
        utilization_percent
    );
}

fn generate_report_body(report_data: &HashMap<NodeState, ReportGroup>, 
    sorted_states: Vec<&NodeState>, 
    max_state_width: usize, 
    widths: (usize, usize, usize, usize),
    no_color: bool,
    show_node_names: bool,
) -> ReportLine {
    
    let max_alloc_cpu_width = widths.0;
    let max_total_cpu_width = widths.1;
    let max_alloc_gpu_width = widths.2;
    let max_total_gpu_width = widths.3;


    let mut total_line = ReportLine::default();
    for state in sorted_states {
        if let Some(group) = report_data.get(state) {
            total_line.node_count += group.summary.node_count;
            total_line.total_cpus += group.summary.total_cpus;
            total_line.alloc_cpus += group.summary.alloc_cpus;
            total_line.total_gpus += group.summary.total_gpus;
            total_line.alloc_gpus += group.summary.alloc_gpus;

            let uncolored_str = state.to_string();
            let padding = " ".repeat(max_state_width.saturating_sub(uncolored_str.len()));
            
            let colored_str = {
                let colorize = |s: &NodeState, text: String| -> ColoredString {
                    if no_color { return text.normal(); }
                    match s {
                        NodeState::Idle => text.green(),
                        NodeState::Mixed => text.cyan(),
                        NodeState::Allocated => text.yellow(),
                        NodeState::Down => text.red(),
                        NodeState::Error => text.magenta(),
                        _ => text.white(),
                    }
                };
                if let NodeState::Compound { base, flags } = state {
                    let base_str = base.to_string();
                    let flags_str = format!("+{}", flags.join("+").to_uppercase());
                    format!("{}{}", colorize(base, base_str), flags_str)
                } else {
                    colorize(state, uncolored_str.clone()).to_string()
                }
            };

            let cpu_str = format_stat_column(group.summary.alloc_cpus as u64, group.summary.total_cpus as u64, max_alloc_cpu_width, max_total_cpu_width);
            let gpu_str = if group.summary.total_gpus > 0 {
                format_stat_column(group.summary.alloc_gpus, group.summary.total_gpus, max_alloc_gpu_width, max_total_gpu_width)
            } else {
                let total_width = max_alloc_gpu_width + max_total_gpu_width;
                format!("{:^width$}", " -  ", width = total_width)
            };
            
            // Use a two-step print to ensure alignment with colored text.
            print!("{}{}", colored_str, padding);

            println!("{:>5}  {}  {}  {}", 
                group.summary.node_count, 
                cpu_str, 
                gpu_str, 
                if show_node_names {
                    fi_slurm::parser::compress_hostlist(&group.summary.node_names)
                } else {
                    "".to_string()
                },
            );


            let mut sorted_subgroups: Vec<&String> = group.subgroups.keys().collect();
            sorted_subgroups.sort();

            for subgroup_name in sorted_subgroups {
                if let Some(subgroup_line) = group.subgroups.get(subgroup_name) {
                    let sub_cpu_str = format_stat_column(subgroup_line.alloc_cpus as u64, subgroup_line.total_cpus as u64, max_alloc_cpu_width, max_total_cpu_width);
                    let sub_gpu_str = if subgroup_line.total_gpus > 0 {
                        format_stat_column(subgroup_line.alloc_gpus, subgroup_line.total_gpus, max_alloc_gpu_width, max_total_gpu_width)
                    } else {
                        let total_width = max_alloc_gpu_width + max_total_gpu_width;
                        format!("{:^width$}", " -  ", width = total_width)
                    };
                    
                    let indented_name = format!("  {}", subgroup_name);
                    let sub_padding = " ".repeat(max_state_width.saturating_sub(indented_name.len()));

                    print!("{}{}", indented_name, sub_padding);
    
                    println!("{:>5}  {}  {}  {}", 
                        subgroup_line.node_count, 
                        sub_cpu_str, 
                        sub_gpu_str, 
                        if show_node_names {
                            fi_slurm::parser::compress_hostlist(&subgroup_line.node_names)
                        } else {
                            "".to_string()
                        }
                    );
                }
            }
        }
    }
    total_line
}
