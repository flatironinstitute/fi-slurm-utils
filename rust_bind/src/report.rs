use crate::nodes::{NodeState, SlurmNodes};
use crate::jobs::SlurmJobs;
use std::collections::HashMap;

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
    nodes: &SlurmNodes,
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<String, Vec<u32>>,
) -> ReportData {
    let mut report_data = ReportData::new();

    // Iterate through every node we loaded from Slurm.
    for node in nodes.nodes.values() {
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
        for (gres_name, &count) in &node.allocated_gres {
            if gres_name.starts_with("gpu") {
                group.summary.alloc_gpus += count;
            }
        }
        
        // --- Feature Subgroup Logic ---
        if let Some(feature) = node.features.first() {
            let subgroup_line = group.subgroups.entry(feature.clone()).or_default();
            subgroup_line.node_count += 1;
            subgroup_line.total_cpus += node.cpus as u32;
            subgroup_line.alloc_cpus += alloc_cpus_for_node;
            // The feature subgroup line shows stats for all GPUs on the node.
            for (gres_name, &count) in &node.configured_gres {
                if gres_name.starts_with("gpu") {
                    subgroup_line.total_gpus += count;
                }
            }
            for (gres_name, &count) in &node.allocated_gres {
                if gres_name.starts_with("gpu") {
                    subgroup_line.alloc_gpus += count;
                }
            }
        }

        // --- GRES Subgroup Logic ---
        // Replicating the original script's behavior, where a node can contribute
        // to multiple GRES subgroups.
        for gres_name in node.configured_gres.keys() {
            let subgroup_line = group.subgroups.entry(gres_name.clone()).or_default();
            subgroup_line.node_count += 1;
            subgroup_line.total_cpus += node.cpus as u32;
            subgroup_line.alloc_cpus += alloc_cpus_for_node;
        }
        // We handle allocated GRES separately to ensure we only add to subgroups
        // that have a configured count.
        for (gres_name, allocated_count) in &node.allocated_gres {
            if let Some(subgroup_line) = group.subgroups.get_mut(gres_name) {
                 subgroup_line.alloc_gpus += *allocated_count;
            }
        }
    }

    report_data
}



//pub fn build_report(
//    nodes: &SlurmNodes,
//    jobs: &SlurmJobs,
//    node_to_job_map: &HashMap<String, Vec<u32>>,
//) -> ReportData {
//    let mut report_data = ReportData::new();
//
//    // Iterate through every node we loaded from Slurm.
//    for node in nodes.nodes.values() {
//        // --- Calculate Allocated CPUs for this specific node ---
//        // Sum the CPUs from all jobs running on this node.
//        let alloc_cpus_for_node = if let Some(job_ids) = node_to_job_map.get(&node.name) {
//            job_ids
//                .iter()
//                .filter_map(|job_id| jobs.jobs.get(job_id)) // Look up each job by its ID
//                .map(|job| job.num_cpus) // Get its CPU count
//                .sum() // Sum them all up
//        } else {
//            0 // No jobs on this node, so 0 allocated CPUs.
//        };
//
//        // A node reporting as ALLOCATED might only be partially used. If so, we
//        // categorize it as MIXED for a more accurate report.
//        let derived_state = if alloc_cpus_for_node > 0 && alloc_cpus_for_node < node.cpus as u32 {
//            NodeState::Mixed
//        } else {
//            // Otherwise, we trust the state reported by Slurm.
//            node.state.clone()
//        };
//
//
//        // Get the report group for the node's current state.
//        let group = report_data.entry(derived_state).or_default();
//
//        // --- Update the Main Summary Line for the Group ---
//        group.summary.node_count += 1;
//        group.summary.total_cpus += node.cpus as u32;
//        group.summary.alloc_cpus += alloc_cpus_for_node;
//        for (gres_name, &count) in &node.configured_gres {
//            if gres_name.starts_with("gpu") {
//                group.summary.total_gpus += count;
//            }
//        }
//        for (gres_name, &count) in &node.allocated_gres {
//            if gres_name.starts_with("gpu") {
//                group.summary.alloc_gpus += count;
//            }
//        }
//
//        // --- Feature Subgroup Logic ---
//        if let Some(feature) = node.features.first() {
//            let subgroup_line = group.subgroups.entry(feature.clone()).or_default();
//            subgroup_line.node_count += 1;
//            subgroup_line.total_cpus += node.cpus as u32;
//            subgroup_line.alloc_cpus += alloc_cpus_for_node;
//            // The feature subgroup line shows stats for all GPUs on the node.
//            for (gres_name, &count) in &node.configured_gres {
//                if gres_name.starts_with("gpu") {
//                    subgroup_line.total_gpus += count;
//                }
//            }
//            for (gres_name, &count) in &node.allocated_gres {
//                if gres_name.starts_with("gpu") {
//                    subgroup_line.alloc_gpus += count;
//                }
//            }
//        }
//
//        // --- GRES Subgroup Logic ---
//        // Replicating the original script's behavior, where a node can contribute
//        // to multiple GRES subgroups.
//        for gres_name in node.configured_gres.keys() {
//            let subgroup_line = group.subgroups.entry(gres_name.clone()).or_default();
//            subgroup_line.node_count += 1;
//            subgroup_line.total_cpus += node.cpus as u32;
//            subgroup_line.alloc_cpus += alloc_cpus_for_node;
//        }
//        // We handle allocated GRES separately to ensure we only add to subgroups
//        // that have a configured count.
//        for gres_name in node.allocated_gres.keys() {
//            if let Some(subgroup_line) = group.subgroups.get_mut(gres_name) {
//                 subgroup_line.alloc_gpus += node.allocated_gres[gres_name];
//            }
//        }
//    }
//
//    report_data
//}

/// Formats and prints the aggregated report data to the console.
pub fn print_report(report_data: &ReportData) {
    // Helper function to format the CPU and GPU stats for a single line.
    fn format_stats_line(line: &ReportLine) -> String {
        let cpu_stats = format!("({:>5}/{:>5})", line.total_cpus, line.alloc_cpus);
        let gpu_stats = if line.total_gpus > 0 || line.alloc_gpus > 0 {
            format!(" ({:>4}/{:>4})", line.total_gpus, line.alloc_gpus)
        } else {
            String::new()
        };
        format!("{}{}", cpu_stats, gpu_stats)
    }

    // Define a custom sort order for the top-level states.
    let state_order: HashMap<NodeState, usize> = [
        (NodeState::Idle, 0),
        (NodeState::Mixed, 1),
        (NodeState::Allocated, 2),
        (NodeState::Down, 3),
        (NodeState::Future, 4),
        (NodeState::Error, 5),
        (NodeState::Unknown("Unknown Node".to_string()), 6),
    ]
    .iter()
    .cloned()
    .collect();

    let mut sorted_states: Vec<&NodeState> = report_data.keys().collect();
    sorted_states.sort_by_key(|a| state_order.get(a).unwrap_or(&99));

    // --- Print Main Report ---
    for state in sorted_states {
        if let Some(group) = report_data.get(state) {
            // Print the main summary line for the state.
            println!(
                "{:<17}\t{:>4} {}",
                format!("{:?}", state).to_uppercase(),
                group.summary.node_count,
                format_stats_line(&group.summary)
            );

            // Print the subgroups.
            let mut sorted_subgroups: Vec<&String> = group.subgroups.keys().collect();
            sorted_subgroups.sort();

            for subgroup_name in sorted_subgroups {
                if let Some(subgroup_line) = group.subgroups.get(subgroup_name) {
                    println!(
                        "  {:<15}\t{:>4} {}",
                        subgroup_name,
                        subgroup_line.node_count,
                        format_stats_line(subgroup_line)
                    );
                }
            }
        }
    }
    
    // --- Print Grand Totals ---
    let mut total_line = ReportLine::default();
    for group in report_data.values() {
        total_line.node_count += group.summary.node_count;
        total_line.total_cpus += group.summary.total_cpus;
        total_line.alloc_cpus += group.summary.alloc_cpus;
        total_line.total_gpus += group.summary.total_gpus;
        total_line.alloc_gpus += group.summary.alloc_gpus;
    }
    
    println!("-----------------------------------------------------");
    println!(
        "Total {:<10}\t{:>4} {}",
        total_line.node_count,
        "", // Empty space for alignment
        format_stats_line(&total_line)
    );
}


