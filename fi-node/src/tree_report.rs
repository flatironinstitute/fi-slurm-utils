use crate::jobs::SlurmJobs;
use crate::nodes::{Node, NodeState};
use colored::*;
use std::collections::HashMap;

// Data Structures for the Tree Report

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


// Aggregation Logic

/// Helper function to determine if a node is available for new work.
fn is_node_available(state: &NodeState) -> bool {
    match state {
        NodeState::Idle => true,
        NodeState::Compound { base, flags } => {
            if **base == NodeState::Idle {
                // Node is idle, but check for disqualifying flags.
                !flags.iter().any(|flag| flag == "MAINT" || flag == "DOWN" || flag == "DRAIN" || flag == "INVALID_REG")
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

/// Recursively calculates the maximum width needed for the feature name column.
fn calculate_max_width(tree_node: &TreeNode, prefix_len: usize) -> usize {
    let mut path_parts = vec![tree_node.name.as_str()];
    let mut current_node = tree_node;
    while current_node.children.len() == 1 {
        let single_child = current_node.children.values().next().unwrap();
        path_parts.push(single_child.name.as_str());
        current_node = single_child;
    }
    let collapsed_name = path_parts.join(", ");
    let current_width = prefix_len + collapsed_name.len() + 4; // +4 for "└── "

    current_node
        .children
        .values()
        .map(|child| calculate_max_width(child, prefix_len + 4))
        .max()
        .unwrap_or(0)
        .max(current_width)
}

// /// The public entry point for printing the tree report
// pub fn print_tree_report(root: &TreeReportData, no_color: bool) {
//     let node_bar = create_avail_bar(root.stats.idle_nodes, root.stats.total_nodes, 30, 
//         if no_color {Color::White} else {Color::Green});
//     let cpu_bar = create_avail_bar(root.stats.idle_cpus, root.stats.total_cpus, 30, 
//         if no_color {Color::White} else {Color::Cyan});
//
//     // Calculate padding for alignment
//     let node_text = format!("Nodes: {}/{} Avail", root.stats.idle_nodes, root.stats.total_nodes);
//     let cpu_text = format!("Processors:  {}/{} Avail", root.stats.idle_cpus, root.stats.total_cpus);
//     let max_text_width = node_text.len().max(cpu_text.len());
//
//     let node_padding = " ".repeat(max_text_width - node_text.len());
//     let cpu_padding = " ".repeat(max_text_width - cpu_text.len());
//
//     println!(
//         "{}: {}{} {}",
//         root.name.bold(),
//         node_text,
//         node_padding,
//         node_bar
//     );
//      println!(
//         "{}  {}{} {}",
//         " ".repeat(root.name.len()),
//         cpu_text,
//         cpu_padding,
//         cpu_bar
//     );
//     println!(" ");
//
//     let mut sorted_children: Vec<_> = root.children.values().collect();
//     sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
//
//     for (i, child) in sorted_children.iter().enumerate() {
//         let is_last = i == sorted_children.len() - 1;
//         print_node_recursive(child, "", is_last, no_color);
//     }
// }

/// The public entry point for printing the tree report.
pub fn print_tree_report(root: &TreeReportData, no_color: bool) {
    println!("--- Feature Tree Report ---\n");

    // --- Pass 1: Calculate dynamic column width ---
    let max_width = calculate_max_width(root, 0) + 2;

    // --- Print Headers ---
    println!(
        "{:<width$} {:>18} {:>18}",
        "FEATURE".bold(), "NODES (Avail/Total)".bold(), "CPUs (Avail/Total)".bold(), width = max_width
    );
    println!("{}", "-".repeat(max_width + 40));

    // --- Print Root/Total line ---
    let node_text = format!("{}/{}", root.stats.idle_nodes, root.stats.total_nodes);
    let cpu_text = format!("{}/{}", root.stats.idle_cpus, root.stats.total_cpus);
    println!(
        "{:<width$} {:>18} {:>18}",
        root.name.bold(), node_text, cpu_text, width = max_width
    );

    // --- Kick off recursive printing for children ---
    let mut sorted_children: Vec<_> = root.children.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
    for (i, child) in sorted_children.iter().enumerate() {
        let is_last = i == sorted_children.len() - 1;
        print_node_recursive(child, "", is_last, no_color, max_width);
    }
}


// /// Recursively prints a node and its children to form the tree structure
// fn print_node_recursive(tree_node: &TreeNode, prefix: &str, is_last: bool, no_color: bool) {
//     let mut path_parts = vec![tree_node.name.as_str()];
//     let mut current_node = tree_node;
//
//     while current_node.children.len() == 1 {
//         let single_child = current_node.children.values().next().unwrap();
//         path_parts.push(single_child.name.as_str());
//         current_node = single_child;
//     }
//
//     let collapsed_name = path_parts.join(", ");
//     let connector = if is_last { "└──" } else { "├──" };
//
//     // FIX: Print the feature name line *without* a leading newline.
//     // The separation is now handled by the parent before the recursive call.
//     println!("{}{}{}", prefix, connector, collapsed_name.bold());
//
//     let child_prefix = if is_last { "    " } else { "│   " };
//
//     let node_text = format!("Nodes: {}/{}", current_node.stats.idle_nodes, current_node.stats.total_nodes);
//     let cpu_text = format!("Processors:  {}/{}", current_node.stats.idle_cpus, current_node.stats.total_cpus);
//     let max_text_width = node_text.len().max(cpu_text.len()) + " Avail".len();
//
//     let node_padding = " ".repeat(max_text_width - (node_text.len() + " Avail".len()));
//     let cpu_padding = " ".repeat(max_text_width - (cpu_text.len() + " Avail".len()));
//
//     let node_bar = create_avail_bar(current_node.stats.idle_nodes, current_node.stats.total_nodes, 30, 
//         if no_color {Color::White} else {Color::Green});
//     let cpu_bar = create_avail_bar(current_node.stats.idle_cpus, current_node.stats.total_cpus, 30, 
//         if no_color {Color::White} else {Color::Cyan});
//
//     println!(
//         "{}{} {} {} {}",
//         prefix, child_prefix, node_text, node_padding, node_bar
//     );
//     println!(
//         "{}{} {} {} {}",
//         prefix, child_prefix, cpu_text, cpu_padding, cpu_bar
//     );   
//
//     let mut sorted_children: Vec<_> = current_node.children.values().collect();
//     sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
//
//     // FIX: Before iterating through children, print the connecting pipe if needed.
//     //if !sorted_children.is_empty() {
//     //    println!("{}{}", prefix, child_prefix);
//     //}
//
//     println!("{}{}", prefix, child_prefix);
//
//     let full_child_prefix = format!("{}{}", prefix, child_prefix);
//     for (i, child) in sorted_children.iter().enumerate() {
//         let is_child_last = i == sorted_children.len() - 1;
//         print_node_recursive(child, &full_child_prefix, is_child_last, no_color);
//     }
// }

/// Recursively prints a node and its children to form the tree structure.
fn print_node_recursive(tree_node: &TreeNode, prefix: &str, is_last: bool, no_color: bool, max_width: usize) {
    let mut path_parts = vec![tree_node.name.as_str()];
    let mut current_node = tree_node;

    while current_node.children.len() == 1 {
        let single_child = current_node.children.values().next().unwrap();
        path_parts.push(single_child.name.as_str());
        current_node = single_child;
    }

    let collapsed_name = path_parts.join(", ");
    let connector = if is_last { "└──" } else { "├──" };
    let display_name = format!("{}{}{}", prefix, connector, collapsed_name);

    // Prepare stats text
    let node_text = format!("{}/{}", current_node.stats.idle_nodes, current_node.stats.total_nodes);
    let cpu_text = format!("{}/{}", current_node.stats.idle_cpus, current_node.stats.total_cpus);

    // Print the single, aligned line for this node
    println!(
        "{:<width$} {:>18} {:>18}",
        display_name.bold(),
        node_text,
        cpu_text,
        width = max_width
    );
    
    // Recurse on the remaining branches
    let full_child_prefix = format!("{}{}", prefix, if is_last { "    " } else { "│   " });
    let mut sorted_children: Vec<_> = current_node.children.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
    
    for (i, child) in sorted_children.iter().enumerate() {
        let is_child_last = i == sorted_children.len() - 1;
        print_node_recursive(child, &full_child_prefix, is_child_last, no_color, max_width);
    }
}


