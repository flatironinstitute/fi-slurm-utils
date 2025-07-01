use crate::nodes::{NodeState, Node};
use crate::jobs::SlurmJobs;
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
}

impl ReportLine {
    /// A helper method to add the stats from a single `Node` to this line
    pub fn add_node_stats(&mut self, node: &crate::nodes::Node) {
        self.node_count += 1;
        self.total_cpus += node.cpus as u32;
        // In older Slurm versions, allocated CPUs might not be directly available
        // and may need to be calculated from jobs. For now, we assume a placeholder.
        // self.alloc_cpus += node.allocated_cpus as u32; 

        if let Some(gpu) = &node.gpu_info {
            self.total_gpus += gpu.total_gpus;
            self.alloc_gpus += gpu.allocated_gpus;
        }
    }
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
    node_to_job_map: &HashMap<String, Vec<u32>>,
) -> ReportData {
    let mut report_data = ReportData::new();

    // Iterate through every node we loaded from Slurm
    // for node in nodes.nodes.values() 
    for &node in nodes {
        // Sum the CPUs from all jobs running on this node
        let alloc_cpus_for_node: u32 = if let Some(job_ids) = node_to_job_map.get(&node.name) {
            job_ids
                .iter()
                .filter_map(|job_id| jobs.jobs.get(job_id)) // Look up each job by its ID
                .map(|job| {
                    // Correctly distribute the job's total CPUs across its nodes
                    if job.num_nodes > 0 {
                        // This prevents division by zero and correctly handles single-node job.
                        job.num_cpus / job.num_nodes
                    } else {
                        job.num_cpus
                    }
                })
                .sum()
        } else {
            0 // No jobs on this node, so 0 allocated CPUs
        };

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
        }

        // This is a CPU-only Node
        // Fall back to using the first feature as its identity
        if let Some(feature) = node.features.first() {
            let subgroup_line = group.subgroups.entry(feature.clone()).or_default();
            
            // Add its stats. GPU counts will naturally be zero
            subgroup_line.node_count += 1;
            subgroup_line.total_cpus += node.cpus as u32;
            subgroup_line.alloc_cpus += alloc_cpus_for_node;
        }
    }
    report_data
}

/// Formats and prints the aggregated report data to the console
pub fn print_report(report_data: &ReportData, no_color: bool) {
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
    let cpu_col_width = max_alloc_cpu_width + max_total_cpu_width + 3; // +3 for " / "
    let gpu_col_width = max_alloc_gpu_width + max_total_gpu_width + 3;
    
    println!(
        "{:<width$} {:>5} {:>cpu_w$} {:>gpu_w$}",
        "STATE".bold(), "COUNT".bold(), cpu_header.bold(), gpu_header.bold(),
        width = max_state_width,
        cpu_w = cpu_col_width,
        gpu_w = gpu_col_width,
    );
    println!("{}", "-".repeat(max_state_width + cpu_col_width + gpu_col_width + 7));

    // Generate and Print Report Body
    let total_line = generate_report_body(report_data, sorted_states, max_state_width, max_alloc_cpu_width, max_total_cpu_width, max_alloc_gpu_width, max_total_gpu_width, no_color);

    // Print Totals and Utilization Bars
    println!("{}", "-".repeat(max_state_width + cpu_col_width + gpu_col_width + 7));
    let total_cpu_str = format_stat_column(total_line.alloc_cpus as u64, total_line.total_cpus as u64, max_alloc_cpu_width, max_total_cpu_width);
    let total_gpu_str = format_stat_column(total_line.alloc_gpus, total_line.total_gpus, max_alloc_gpu_width, max_total_gpu_width);
    let total_padding = " ".repeat(max_state_width - "TOTAL".len());
    print!("{}{}", "TOTAL".bold(), total_padding);
    println!("{:>5} {} {}", total_line.node_count, total_cpu_str, total_gpu_str);

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

#[allow(dead_code)]
enum BarColor {
    Red,
    Green,
    Cyan
}

impl BarColor {
    pub fn apply_color(self, text: String) -> ColoredString {
        match self {
            BarColor::Cyan => text.cyan(),
            BarColor::Red => text.red(),
            BarColor::Green => text.green()
        }
    }
}

fn print_utilization(utilization_percent: f64, bar_width: usize, bar_color: BarColor, name: &str, no_color: bool) {
        let filled_chars = (bar_width as f64 * (utilization_percent / 100.0)).round() as usize;

        let filled_bar = if no_color {"█".repeat(filled_chars).white()} else {bar_color.apply_color("█".repeat(filled_chars))};
        let empty_chars = bar_width.saturating_sub(filled_chars);        
        let empty_bar = "░".repeat(empty_chars).normal();

        println!("Overall {} Utilization: \n [{}{}] {:.1}%", name, filled_bar, empty_bar, utilization_percent);
}

fn generate_report_body(report_data: &HashMap<NodeState, ReportGroup>, sorted_states: Vec<&NodeState>, max_state_width: usize, max_alloc_cpu_width: usize, max_total_cpu_width: usize, max_alloc_gpu_width: usize, max_total_gpu_width: usize, no_color: bool) -> ReportLine {
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
                let total_width = max_alloc_gpu_width + max_total_gpu_width + 3;
                format!("{:^width$}", "-", width = total_width)
            };
            
            // Use a two-step print to ensure alignment with colored text.
            print!("{}{}", colored_str, padding);
            println!("{:>5} {} {}", group.summary.node_count, cpu_str, gpu_str);

            let mut sorted_subgroups: Vec<&String> = group.subgroups.keys().collect();
            sorted_subgroups.sort();

            for subgroup_name in sorted_subgroups {
                if let Some(subgroup_line) = group.subgroups.get(subgroup_name) {
                    let sub_cpu_str = format_stat_column(subgroup_line.alloc_cpus as u64, subgroup_line.total_cpus as u64, max_alloc_cpu_width, max_total_cpu_width);
                    let sub_gpu_str = if subgroup_line.total_gpus > 0 {
                        format_stat_column(subgroup_line.alloc_gpus, subgroup_line.total_gpus, max_alloc_gpu_width, max_total_gpu_width)
                    } else {
                        let total_width = max_alloc_gpu_width + max_total_gpu_width + 3;
                        format!("{:^width$}", "-", width = total_width)
                    };
                    
                    let indented_name = format!("  {}", subgroup_name);
                    let sub_padding = " ".repeat(max_state_width.saturating_sub(indented_name.len()));

                    print!("{}{}", indented_name, sub_padding);
                    println!("{:>5} {} {}", subgroup_line.node_count, sub_cpu_str, sub_gpu_str);
                }
            }
        }
    }
    total_line
}
