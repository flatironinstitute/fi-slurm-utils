use crate::jobs::SlurmJobs;
use crate::nodes::{Node, NodeState};
use colored::*;
use std::collections::HashMap;

// --- Data Structures for the Tree Report ---

/// Represents a single node in the feature hierarchy tree.
#[derive(Default, Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub stats: ReportLine,
    pub children: HashMap<String, TreeNode>,
}

/// A simplified version of the ReportLine from the detailed report.
#[derive(Default, Debug, Clone)]
pub struct ReportLine {
    pub total_nodes: u32,
    pub idle_nodes: u32, // New: For tracking available nodes
    pub total_cpus: u32,
    pub idle_cpus: u32, // New: For tracking available CPUs
    pub alloc_cpus: u32,
}

pub type TreeReportData = TreeNode;


// --- Aggregation Logic ---

/// Helper function to determine if a node is available for new work.
fn is_node_available(state: &NodeState) -> bool {
    match state {
        NodeState::Idle => true,
        NodeState::Compound { base, flags } => {
            if **base == NodeState::Idle {
                // Node is idle, but check for disqualifying flags.
                !flags.iter().any(|flag| flag == "MAINT" || flag == "RES")
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Builds a hierarchical tree report from a flat list of Slurm nodes.
pub fn build_tree_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<String, Vec<u32>>,
    feature_filter: &[String],
) -> TreeReportData {
    let mut root = TreeNode {
        name: "TOTAL".to_string(),
        ..Default::default()
    };

    for &node in nodes {
        let alloc_cpus_for_node: u32 = if let Some(job_ids) = node_to_job_map.get(&node.name) {
            job_ids.iter().filter_map(|id| jobs.jobs.get(id)).map(|j| j.num_cpus / j.num_nodes.max(1)).sum()
        } else {
            0
        };
        let is_available = is_node_available(&node.state);

        // --- Update Grand Total Stats ---
        root.stats.total_nodes += 1;
        root.stats.total_cpus += node.cpus as u32;
        root.stats.alloc_cpus += alloc_cpus_for_node;
        if is_available {
            root.stats.idle_nodes += 1;
            root.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
        }

        // --- Tree Building Logic ---
        if feature_filter.is_empty() {
            // --- Default Behavior: Build tree from full feature list ---
            let mut current_level = &mut root;
            for feature in &node.features {
                current_level = current_level.children.entry(feature.clone()).or_default();
                current_level.name = feature.clone();
                // Add stats to this branch
                current_level.stats.total_nodes += 1;
                current_level.stats.total_cpus += node.cpus as u32;
                current_level.stats.alloc_cpus += alloc_cpus_for_node;
                if is_available {
                    current_level.stats.idle_nodes += 1;
                    current_level.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                }
            }
        } else {
            // --- Filtered Behavior: Make filtered features the top level ---
            for filter in feature_filter {
                if node.features.contains(filter) {
                    let mut current_level = root.children.entry(filter.clone()).or_default();
                    current_level.name = filter.clone();
                    // Add stats to this top-level branch
                    current_level.stats.total_nodes += 1;
                    current_level.stats.total_cpus += node.cpus as u32;
                    current_level.stats.alloc_cpus += alloc_cpus_for_node;
                    if is_available {
                       current_level.stats.idle_nodes += 1;
                       current_level.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                    }
                    
                    // Build the sub-branch from the *remaining* features
                    for feature in node.features.iter().filter(|f| *f != filter) {
                         current_level = current_level.children.entry(feature.clone()).or_default();
                         current_level.name = feature.clone();
                         // Add stats to the sub-branch
                         current_level.stats.total_nodes += 1;
                         current_level.stats.total_cpus += node.cpus as u32;
                         current_level.stats.alloc_cpus += alloc_cpus_for_node;
                         if is_available {
                            current_level.stats.idle_nodes += 1;
                            current_level.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                         }
                    }
                }
            }
        }
    }
    root
}


// Display Logic


/// Creates a colored bar string for available resources (nodes or CPUs)
fn create_avail_bar(current: u32, total: u32, width: usize, color: Color) -> String {
    if total == 0 {
        return "N/A".dimmed().to_string();
    }
    let percentage = current as f64 / total as f64;
    let filled_len = (width as f64 * percentage).round() as usize;

    let filled = "■".repeat(filled_len).color(color);
    let empty = " ".repeat(width.saturating_sub(filled_len));

    format!("[{}{}]", filled, empty)
}


/// The public entry point for printing the tree report
pub fn print_tree_report(root: &TreeReportData) {
    println!("--- Feature Tree Report ---\n");
    let node_bar = create_avail_bar(root.stats.idle_nodes, root.stats.total_nodes, 30, Color::Green);
    let cpu_bar = create_avail_bar(root.stats.idle_cpus, root.stats.total_cpus, 30, Color::Cyan);
    println!(
        "{}: {}/{} Nodes Avail {}, {}/{} CPUs Avail {}",
        root.name.bold(),
        root.stats.idle_nodes,
        root.stats.total_nodes,
        node_bar,
        root.stats.idle_cpus,
        root.stats.total_cpus,
        cpu_bar
    );
    
    let mut sorted_children: Vec<_> = root.children.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));

    for (i, child) in sorted_children.iter().enumerate() {
        let is_last = i == sorted_children.len() - 1;
        print_node_recursive(child, "", is_last);
    }
}

/// Recursively prints a node and its children to form the tree structure
fn print_node_recursive(tree_node: &TreeNode, prefix: &str, is_last: bool) {
    let mut path_parts = vec![tree_node.name.as_str()];
    let mut current_node = tree_node;

    while current_node.children.len() == 1 {
        let single_child = current_node.children.values().next().unwrap();
        path_parts.push(single_child.name.as_str());
        current_node = single_child;
    }

    let collapsed_name = path_parts.join("/");
    let connector = if is_last { "└──" } else { "├──" };
    println!("\n{}{}{}", prefix, connector, collapsed_name.bold());

    let child_prefix = if is_last { "    " } else { "│   " };
    
    // Now both bars represent availability
    let node_bar = create_avail_bar(current_node.stats.idle_nodes, current_node.stats.total_nodes, 30, Color::Green);
    let cpu_bar = create_avail_bar(current_node.stats.idle_cpus, current_node.stats.total_cpus, 30, Color::Cyan);
    
    println!(
        "{}{}Nodes: {}/{} Avail {}",
        prefix, child_prefix, current_node.stats.idle_nodes, current_node.stats.total_nodes, node_bar
    );
    println!(
        "{}{}CPUs:  {}/{} Avail {}",
        prefix, child_prefix, current_node.stats.idle_cpus, current_node.stats.total_cpus, cpu_bar
    );

    let full_child_prefix = format!("{}{}", prefix, child_prefix);
    let mut sorted_children: Vec<_> = current_node.children.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
    
    for (i, child) in sorted_children.iter().enumerate() {
        let is_child_last = i == sorted_children.len() - 1;
        print_node_recursive(child, &full_child_prefix, is_child_last);
    }
}

