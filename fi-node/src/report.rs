use crate::nodes::{NodeState, Node};
use crate::jobs::SlurmJobs;
use std::collections::HashMap;
use colored::*;

/// Represents the aggregated statistics for a single line in the final report.
///
/// This struct is used for both the main summary lines (e.g., "IDLE 197...")
/// and the indented subgroup lines (e.g., "  genoa  8...").
///
/// `#[derive(Default)]` allows us to easily create a new, zeroed-out instance.
#[derive(Default, Debug, Clone)]
pub struct ReportLine {
    pub node_count: u32,
    pub total_cpus: u32,
    pub alloc_cpus: u32,
    pub total_gpus: u64,
    pub alloc_gpus: u64,
}

impl ReportLine {
    /// A helper method to add the stats from a single `Node` to this line.
    /// This will be useful in our aggregation loop.
    pub fn add_node_stats(&mut self, node: &crate::nodes::Node) {
        self.node_count += 1;
        self.total_cpus += node.cpus as u32;
        // In older Slurm versions, allocated CPUs might not be directly available
        // and may need to be calculated from jobs. For now, we assume a placeholder.
        // self.alloc_cpus += node.allocated_cpus as u32; 

        // Sum up the GPUs from the GRES maps.
        for (gres_name, &count) in &node.configured_gres {
            if gres_name.starts_with("gpu") {
                self.total_gpus += count;
            }
        }
        // commented out, since it turns out that gres_used is empty most or all the time
        for (gres_name, &count) in &node.allocated_gres {
            if gres_name.starts_with("gpu") {
                self.alloc_gpus += count;
            }
        }
    }
}


/// Represents a top-level group in the report, categorized by a `NodeState`.
///
/// For example, this would hold all the data for the "IDLE" or "MIXED" sections.
#[derive(Default, Debug, Clone)]
pub struct ReportGroup {
    /// The aggregated statistics for the main summary line of this group.
    pub summary: ReportLine,
    /// A map holding the statistics for each subgroup, keyed by feature or GRES name.
    /// For example: `{"genoa": ReportLine, "h100": ReportLine}`
    pub subgroups: HashMap<String, ReportLine>,
}

/// The final data structure that holds the entire report, organized by `NodeState`.
///
/// This will be a map from a `NodeState` enum to its corresponding `ReportGroup`.
/// Using a HashMap makes it easy to add stats to the correct group as we
/// iterate through all the nodes.
pub type ReportData = HashMap<NodeState, ReportGroup>;


/// Builds the final report by aggregating data from all nodes.
///
/// This function iterates through every node, categorizes it by its state,
/// and calculates summary statistics for each state and its subgroups.
///
/// # Arguments
///
/// * `nodes` - A reference to the fully loaded `SlurmNodes` collection.
/// * `node_to_job_map` - A map from node names to the jobs running on them.
///
/// # Returns
///
/// A `ReportData` HashMap containing all the aggregated information for display.
pub fn build_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<String, Vec<u32>>,
) -> ReportData {
    let mut report_data = ReportData::new();

    // Iterate through every node we loaded from Slurm.
    // for node in nodes.nodes.values() {
    for &node in nodes {
        // --- Calculate Allocated CPUs for this specific node ---
        // Sum the CPUs from all jobs running on this node.
        let alloc_cpus_for_node: u32 = if let Some(job_ids) = node_to_job_map.get(&node.name) {
            job_ids
                .iter()
                .filter_map(|job_id| jobs.jobs.get(job_id)) // Look up each job by its ID
                .map(|job| {
                    // Correctly distribute the job's total CPUs across its nodes.
                    if job.num_nodes > 0 {
                        // This prevents division by zero and correctly handles single-node jobs.
                        job.num_cpus / job.num_nodes
                    } else {
                        job.num_cpus
                    }
                })
                .sum()
        } else {
            0 // No jobs on this node, so 0 allocated CPUs.
        };

        // --- Derive the true node state ---
        // A node reporting as ALLOCATED might only be partially used. If so, we
        // categorize it as MIXED for a more accurate report.
        let derived_state = if alloc_cpus_for_node > 0 && alloc_cpus_for_node < node.cpus as u32 {
            NodeState::Mixed
        } else {
            // Otherwise, we trust the state reported by Slurm.
            node.state.clone()
        };

        // Get the report group for the node's derived state.
        let group = report_data.entry(derived_state).or_default();

        // --- Update the Main Summary Line for the Group ---
        group.summary.node_count += 1;
        group.summary.total_cpus += node.cpus as u32;
        group.summary.alloc_cpus += alloc_cpus_for_node;
        for (gres_name, &count) in &node.configured_gres {
            if gres_name.starts_with("gpu") {
                group.summary.total_gpus += count;
            }
        }
        // commented out, since it turns out that gres_used is empty most or all the time
        for (gres_name, &count) in &node.allocated_gres {
            if gres_name.starts_with("gpu") {
                group.summary.alloc_gpus += count;
            }
        }
        
        // --- Feature Subgroup Logic ---
        if let Some(feature) = node.features.first() {
            // FIX: Replicate the original script's logic to ignore the generic "gpu"
            // feature, deferring to the more specific GRES subgroups.
            if feature != "gpu" {
                let subgroup_line = group.subgroups.entry(feature.clone()).or_default();
                subgroup_line.node_count += 1;
                subgroup_line.total_cpus += node.cpus as u32;
                subgroup_line.alloc_cpus += alloc_cpus_for_node;
                // FIX: CPU-based features should not aggregate GPU stats. This is
                // handled by the GRES-specific subgroups below.
            }
        }

        // --- GRES Subgroup Logic ---
        for (gres_name, &configured_count) in &node.configured_gres {
            let subgroup_line = group.subgroups.entry(gres_name.clone()).or_default();

            // FIX: Only add the node count if it hasn't been added by the feature logic.
            // This prevents double-counting nodes in the subgroup totals.
            // A simple way is to check if the subgroup is new.
            if subgroup_line.node_count == 0 {
                subgroup_line.node_count = 1; // Or handle more complex multi-gres nodes
            }
            // For simplicity in this example, we assume CPU stats are mainly relevant to the primary feature.
            // A more complex model could distribute these.
            
            // FIX: Add the configured GPU count for THIS specific GRES.
            if gres_name.starts_with("gpu") {
                subgroup_line.total_gpus += configured_count;
            }
        }
        // commented out, since it turns out that gres_used is empty most or all the time
        //// FIX: This second loop remains necessary to add the allocated counts.
        for (gres_name, &allocated_count) in &node.allocated_gres {
            if let Some(subgroup_line) = group.subgroups.get_mut(gres_name) {
                 if gres_name.starts_with("gpu") {
                    subgroup_line.alloc_gpus += allocated_count;
                 }
            }
        }
    }
    report_data
}

/// Formats and prints the aggregated report data to the console.
pub fn print_report(report_data: &ReportData, no_color: bool) {
    let mut max_state_width = "STATE".len();
    for (state, group) in report_data.iter() {
        max_state_width = max_state_width.max(state.to_string().len());
        for subgroup_name in group.subgroups.keys() {
            max_state_width = max_state_width.max(subgroup_name.len() + 2);
        }
    }
    max_state_width += 2;

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

    println!(
        "{:<width$} {:>5} {:>13}",
        "STATE".bold(), "COUNT".bold(), "PROCESSORS (A/T)".bold(),  width = max_state_width
    );
    println!("{}", "-".repeat(max_state_width + 23));

    let mut total_line = ReportLine::default();
    for state in sorted_states {
        if let Some(group) = report_data.get(state) {
            total_line.node_count += group.summary.node_count;
            total_line.total_cpus += group.summary.total_cpus;
            total_line.alloc_cpus += group.summary.alloc_cpus;
            total_line.total_gpus += group.summary.total_gpus;
            total_line.alloc_gpus += group.summary.alloc_gpus;

            let uncolored_str = state.to_string();
            let padding_len = max_state_width.saturating_sub(uncolored_str.len());
            let padding = " ".repeat(padding_len);
            
            let colored_str = match state {
                NodeState::Compound { base, flags } => {
                    let base_str = base.to_string();
                    let flags_str = format!("+{}", flags.join("+").to_uppercase());
                    let colored_base = match **base {
                        NodeState::Idle => if no_color { base_str.white()} else {base_str.green()},
                        NodeState::Mixed => if no_color { base_str.white()} else {base_str.blue()},
                        NodeState::Allocated => if no_color { base_str.white()} else {base_str.yellow()},
                        NodeState::Down => if no_color { base_str.white()} else {base_str.red()},
                        NodeState::Error => if no_color { base_str.white()} else {base_str.magenta()},
                        _ => base_str.cyan(),
                    };
                    format!("{}{}", colored_base, flags_str)
                }
                _ => {
                    match state {
                        NodeState::Idle => if no_color { uncolored_str.white().to_string()} else {uncolored_str.green().to_string()},
                        NodeState::Mixed => if no_color { uncolored_str.white().to_string()} else {uncolored_str.blue().to_string()},
                        NodeState::Allocated => if no_color { uncolored_str.white().to_string()} else {uncolored_str.yellow().to_string()},
                        NodeState::Down => if no_color { uncolored_str.white().to_string()} else {uncolored_str.red().to_string()},
                        NodeState::Error => if no_color { uncolored_str.white().to_string()} else {uncolored_str.magenta().to_string()},
                        _ => uncolored_str,
                    }
                }
            };
            
            let cpu_str = format!("{:>5}/{:<5}", group.summary.alloc_cpus, group.summary.total_cpus);
            let gpu_str = if group.summary.total_gpus > 0 {
                format!("{:>5}/{:<5}", group.summary.alloc_gpus, group.summary.total_gpus)
            } else {
                "-".to_string()
            };
            
            println!("{}{}{:>5} {:>13} {:>13}", colored_str, padding, group.summary.node_count, cpu_str, gpu_str);
            //println!("{}{}{:>5} {:>13}", colored_str, padding, group.summary.node_count, cpu_str);

            let mut sorted_subgroups: Vec<&String> = group.subgroups.keys().collect();
            sorted_subgroups.sort();

            for subgroup_name in sorted_subgroups {
                if let Some(subgroup_line) = group.subgroups.get(subgroup_name) {
                    let sub_cpu_str = format!("{:>5}/{:<5}", subgroup_line.alloc_cpus, subgroup_line.total_cpus);
                    let sub_gpu_str = if subgroup_line.total_gpus > 0 {
                        format!("{:>5}/{:<5}", subgroup_line.alloc_gpus, subgroup_line.total_gpus)
                    } else {
                        "-".to_string()
                    };
                    
                    let indented_name = format!("  {}", subgroup_name);
                    let sub_padding_len = max_state_width.saturating_sub(indented_name.len());
                    let sub_padding = " ".repeat(sub_padding_len);

                    //println!("{}{}{:>5} {:>13}", indented_name, sub_padding, subgroup_line.node_count, sub_cpu_str);
                    println!("{}{}{:>5} {:>13} {:>13}", indented_name, sub_padding, subgroup_line.node_count, sub_cpu_str, sub_gpu_str);
                }
            }
        }
    }
    
    println!("{}", "-".repeat(max_state_width + 23));
    let total_cpu_str = format!("{:>5}/{:<5}", total_line.alloc_cpus, total_line.total_cpus);
    let total_gpu_str = format!("{:>5}/{:<5}", total_line.alloc_gpus, total_line.total_gpus);
    let total_padding = " ".repeat(max_state_width - "TOTAL".len());
    println!("{}{}{:>5} {:>13} {:>13}", "TOTAL".bold(), total_padding, total_line.node_count, total_cpu_str, total_gpu_str);

    println!();

    let utilized_nodes = report_data.iter().fold(0, |acc, (state, group)| {
        let is_utilized = match state {
            NodeState::Allocated | NodeState::Mixed => true,
            NodeState::Compound{ base, ..} => matches!(**base, NodeState::Allocated | NodeState::Mixed),
            _ => false,
        };
        if is_utilized { acc + group.summary.node_count } else { acc }
    });

    if total_line.node_count > 0 {
        let utilization_percent = (utilized_nodes as f64 / total_line.node_count as f64) * 100.0;
        let bar_width: usize = 50;
        let filled_chars = (bar_width as f64 * (utilization_percent / 100.0)).round() as usize;

        let filled_bar = if no_color {"█".repeat(filled_chars).white()} else {"█".repeat(filled_chars).blue()};
        let empty_chars = bar_width.saturating_sub(filled_chars);         
        let empty_bar = "░".repeat(empty_chars).normal();
        
        println!("Overall Node Utilization: \n [{}{}] {:.1}%", filled_bar, empty_bar, utilization_percent);
    }

    if total_line.total_cpus > 0 {
        let utilization_percent = (total_line.alloc_cpus as f64 / total_line.total_cpus as f64) * 100.0;
        let bar_width: usize = 50;
        let filled_chars = (bar_width as f64 * (utilization_percent / 100.0)).round() as usize;
        
        let filled_bar = if no_color {"█".repeat(filled_chars).white()} else {"█".repeat(filled_chars).red()};
        let empty_chars = bar_width.saturating_sub(filled_chars);        
        let empty_bar = if no_color {"░".repeat(empty_chars).white()} else {"░".repeat(empty_chars).green()};

        println!("Overall Processor Utilization: \n [{}{}] {:.1}%", filled_bar, empty_bar, utilization_percent);
    }
}

