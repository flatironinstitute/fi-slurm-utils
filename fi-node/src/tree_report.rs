use crate::jobs::SlurmJobs;
use crate::nodes::{Node, NodeState};
use colored::*;
use std::collections::{HashMap, HashSet};

// Data Structures for the Tree Report

/// Represents a single node in the feature hierarchy tree
#[derive(Default, Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub stats: ReportLine,
    pub children: HashMap<String, TreeNode>,
    pub single_filter: bool, // used to determine whether we are filtering on a single item
}

/// A simplified version of the ReportLine from the detailed report
#[derive(Default, Debug, Clone)]
pub struct ReportLine {
    pub total_nodes: u32,
    pub idle_nodes: u32,
    pub total_cpus: u32,
    pub idle_cpus: u32,
    pub alloc_cpus: u32,
    pub node_names: Vec<String>,
}

pub type TreeReportData = TreeNode;


// Aggregation Logic

/// Helper function to determine if a node is available for new work
fn is_node_available(state: &NodeState) -> bool {
    match state {
        NodeState::Idle => true,
        NodeState::Compound { base, flags } => {
            if **base == NodeState::Idle {
                // Node is idle, but check for disqualifying flags
                !flags.iter().any(|flag| flag == "MAINT" || flag == "DOWN" || flag == "DRAIN" || flag == "INVALID_REG")
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Builds a hierarchical tree report from a flat list of Slurm nodes
pub fn build_tree_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<usize, Vec<u32>>,
    feature_filter: &[String],
    show_hidden_features: bool,
    show_node_names: bool,
) -> TreeReportData {
    let mut root = TreeNode {
        name: "TOTAL".to_string(),
        ..Default::default()
    };

    if feature_filter.len() == 1 {root.single_filter = true};

    let hidden_features: HashSet<&str> = [
        "rocky8", "rocky9", "sxm", "sxm2", "sxm4", "sxm5", "nvlink", "a100", "h100", "v100", "ib",
    ].iter().cloned().collect();

    for &node in nodes {
        let alloc_cpus_for_node: u32 = if let Some(job_ids) = node_to_job_map.get(&node.id) {
            job_ids.iter().filter_map(|id| jobs.jobs.get(id)).map(|j| j.num_cpus / j.num_nodes.max(1)).sum()
        } else {
            0
        };
        let is_available = is_node_available(&node.state);

        // Update Grand Total Stats
        root.stats.total_nodes += 1;
        root.stats.total_cpus += node.cpus as u32;
        root.stats.alloc_cpus += alloc_cpus_for_node;
        if is_available {
            root.stats.idle_nodes += 1;
            root.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
        }
        if show_node_names {
            root.stats.node_names.push(node.name);
        }

        // NEW: Determine which features to use for building the tree based on the flag.
        let features_for_tree: Vec<_> = if show_hidden_features {
            node.features.iter().collect()
        } else {
            node.features.iter().filter(|f| !hidden_features.contains(f.as_str())).collect()
        };
        
        // --- Tree Building Logic ---
        if feature_filter.is_empty() {
            // --- Default Behavior: Build tree from the (potentially filtered) feature list ---
            let mut current_level = &mut root;
            for feature in &features_for_tree {
                current_level = current_level.children.entry(feature.to_string()).or_default();
                current_level.name = feature.to_string();
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
                // IMPORTANT: The check to see if a node belongs under a filter
                // must use the ORIGINAL, unfiltered features.
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

                    // Build the sub-branch from the *remaining* features,
                    // respecting the show_hidden_features flag.
                    for feature in features_for_tree.iter().filter(|f| f.as_str() != filter) {
                        current_level = current_level.children.entry(feature.to_string()).or_default();
                        current_level.name = feature.to_string();
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

#[derive(Default)]
struct ColumnWidths {
    max_idle_nodes: usize,
    max_total_nodes: usize,
    max_idle_cpus: usize,
    max_total_cpus: usize,
}

fn calculate_column_widths(tree_node: &TreeNode) -> ColumnWidths {
    let mut widths = ColumnWidths {
        max_idle_nodes: tree_node.stats.idle_nodes.to_string().len(),
        max_total_nodes: tree_node.stats.total_nodes.to_string().len(),
        max_idle_cpus: tree_node.stats.idle_cpus.to_string().len(),
        max_total_cpus: tree_node.stats.total_cpus.to_string().len(),
    };

    for child in tree_node.children.values() {
        let child_widths = calculate_column_widths(child);
        widths.max_idle_nodes = widths.max_idle_nodes.max(child_widths.max_idle_nodes);
        widths.max_total_nodes = widths.max_total_nodes.max(child_widths.max_total_nodes);
        widths.max_idle_cpus = widths.max_idle_cpus.max(child_widths.max_idle_cpus);
        widths.max_total_cpus = widths.max_total_cpus.max(child_widths.max_total_cpus);
    }

    widths
}

/// based on the pre-calculated maximum widths
fn format_tree_stat_column(current: u32, total: u32, max_current_width: usize, max_total_width: usize) -> String {
    format!(
        "{:>current_w$}/{:>total_w$} ",
        current,
        total,
        current_w = max_current_width,
        total_w = max_total_width
    )
}

/// Creates a colored bar string for available resources (nodes or CPUs)
fn create_avail_bar(current: u32, total: u32, width: usize, color: Color, no_color: bool) -> String {
    if total == 0 {
        // To avoid division by zero and provide clear output for empty categories
        let bar_content = " ".repeat(width);
        return format!("|{}|", bar_content);
    }
    let percentage = current as f64 / total as f64;
    let filled_len = (width as f64 * percentage).round() as usize;

    let filled = "■".repeat(filled_len).color(if no_color { Color::White} else { color });
    let empty = " ".repeat(width.saturating_sub(filled_len));

    format!("|{}{}|", filled, empty)
}

/// Recursively calculates the maximum width needed for the feature name column
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


pub fn print_tree_report(root: &TreeReportData, no_color: bool) {
    // --- Define Headers ---
    const HEADER_FEATURE: &str = "FEATURE (Avail/Total)";
    const HEADER_NODES: &str = "NODES";
    const HEADER_CPUS: &str = "PROCESSORS";
    const HEADER_NODE_AVAIL: &str = "NODES AVAIL.";
    const HEADER_CPU_AVAIL: &str = "PROCESSORS AVAIL.";

    // Calculate Column Widths
    let max_feature_width = calculate_max_width(root, 0).max(HEADER_FEATURE.len()) - 4;
    let bar_width = 20;
    
    let col_widths = calculate_column_widths(root);
    // Correctly calculate the width of the data string (e.g., "123/4567")
    let nodes_data_width = col_widths.max_idle_nodes + col_widths.max_total_nodes + 1; // +1 for "/"
    let cpus_data_width = col_widths.max_idle_cpus + col_widths.max_total_cpus + 1;

    // The final column width is the larger of the header or the data
    let nodes_final_width = nodes_data_width.max(HEADER_NODES.len());
    let cpus_final_width = cpus_data_width.max(HEADER_CPUS.len());
    let bar_final_width = (bar_width + 2).max(HEADER_NODE_AVAIL.len()); // +2 for "||"

    // Determine what to print as the top level
    let (top_level_node, children_to_iterate) = if root.single_filter {
        if let Some(single_child) = root.children.values().next() {
            (single_child, &single_child.children)
        } else {
            (root, &root.children)
        }
    } else {
        (root, &root.children)
    };

    // Print the determined top-level line with Right Alignment for data
    let node_text = format_tree_stat_column(top_level_node.stats.idle_nodes, top_level_node.stats.total_nodes, col_widths.max_idle_nodes, col_widths.max_total_nodes);
    let cpu_text = format_tree_stat_column(top_level_node.stats.idle_cpus, top_level_node.stats.total_cpus, col_widths.max_idle_cpus, col_widths.max_total_cpus);
    let node_bar = create_avail_bar(top_level_node.stats.idle_nodes, top_level_node.stats.total_nodes, bar_width, Color::Green, no_color);
    let cpu_bar = create_avail_bar(top_level_node.stats.idle_cpus, top_level_node.stats.total_cpus, bar_width, Color::Cyan, no_color);

    // Print Headers with Centered Alignment
    println!(
        "{:<feature_w$} {:^nodes_w$} {:^cpus_w$} {:^bar_w$} {:^bar_w$}",
        HEADER_FEATURE.bold(),
        HEADER_NODES.bold(),
        HEADER_CPUS.bold(),
        HEADER_NODE_AVAIL.bold(),
        HEADER_CPU_AVAIL.bold(),
        feature_w = max_feature_width,
        nodes_w = nodes_final_width,
        cpus_w = cpus_final_width,
        bar_w = bar_final_width
    );

    // Print Separator Line
    let total_width = max_feature_width + nodes_final_width + cpus_final_width + bar_final_width * 2 + 6; // +4 for spaces between columns
    println!("{}", "-".repeat(total_width));

    println!(
        "{:<feature_w$} {:>nodes_w$} {:>cpus_w$} {} {}",
        top_level_node.name.bold(),
        node_text,
        cpu_text,
        node_bar,
        cpu_bar,
        feature_w = max_feature_width,
        nodes_w = nodes_final_width,
        cpus_w = cpus_final_width
    );

    // Print the children recursively
    let mut sorted_children: Vec<_> = children_to_iterate.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
    for (i, child) in sorted_children.iter().enumerate() {
        let is_last = i == sorted_children.len() - 1;
        print_node_recursive(child, "", is_last, no_color, (max_feature_width, bar_width, nodes_final_width, cpus_final_width), &col_widths);
    }
}

/// Recursively prints a node and its children to form the tree structure
fn print_node_recursive(
    tree_node: &TreeNode, 
    prefix: &str, 
    is_last: bool, 
    no_color: bool, 
    widths: (usize, usize, usize, usize),
    col_widths: &ColumnWidths,
) {
    let mut path_parts = vec![tree_node.name.as_str()];
    let mut current_node = tree_node;
    
    let max_width = widths.0;
    let bar_width = widths.1;
    let nodes_final_width = widths.2;
    let cpus_final_width = widths.3;

    while current_node.children.len() == 1 {
        let single_child = current_node.children.values().next().unwrap();
        path_parts.push(single_child.name.as_str());
        current_node = single_child;
    }

    let collapsed_name = path_parts.join(", ");
    let connector = if is_last { "└──" } else { "├──" };
    let display_name = format!("{}{}{}", prefix, connector, collapsed_name);

    let node_text = format_tree_stat_column(current_node.stats.idle_nodes, current_node.stats.total_nodes, col_widths.max_idle_nodes, col_widths.max_total_nodes);
    let cpu_text = format_tree_stat_column(current_node.stats.idle_cpus, current_node.stats.total_cpus, col_widths.max_idle_cpus, col_widths.max_total_cpus);
    let node_bar = create_avail_bar(current_node.stats.idle_nodes, current_node.stats.total_nodes, bar_width, Color::Green, no_color);
    let cpu_bar = create_avail_bar(current_node.stats.idle_cpus, current_node.stats.total_cpus, bar_width, Color::Cyan, no_color);
    let node_names = current_node.stats.node_names;

    println!(
        "{:<feature_w$} {:>nodes_w$} {:>cpus_w$} {} {}: {}",
        display_name.bold(),
        node_text,
        cpu_text,
        node_bar,
        cpu_bar,
        feature_w = max_width,
        nodes_w = nodes_final_width,
        cpus_w = cpus_final_width,
        node_names,
    );

    let full_child_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });
    let mut sorted_children: Vec<_> = current_node.children.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));

    for (i, child) in sorted_children.iter().enumerate() {
        let is_child_last = i == sorted_children.len() - 1;
        print_node_recursive(child, &full_child_prefix, is_child_last, no_color, (max_width, bar_width, nodes_final_width, cpus_final_width), col_widths);
    }
}

