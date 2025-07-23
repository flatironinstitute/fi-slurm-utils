use crate::PreemptNodes;
use crate::jobs::SlurmJobs;
use crate::nodes::{Node, NodeState};
use fi_slurm::utils::count_blocks;
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
    pub preempt_nodes: Option<u32>,
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
fn is_node_mixed(state: &NodeState) -> bool {
    match state {
        NodeState::Mixed => true,
        NodeState::Compound { base, flags } => {
            if **base == NodeState::Mixed {
                // Node is mixed, but check for disqualifying flags
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
    preempted_nodes: Option<PreemptNodes>,
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

        let derived_state = if alloc_cpus_for_node > 0 && alloc_cpus_for_node < node.cpus as u32 {
            match &node.state {
                NodeState::Compound { flags, .. } => NodeState::Compound { base: Box::new(NodeState::Mixed), flags: flags.to_vec()},
                _ => NodeState::Mixed,
            }
        } else {
            // Otherwise, we trust the state reported by Slurm
            node.state.clone()
        };


        let is_available = is_node_available(&derived_state);
        let is_mixed = is_node_mixed(&derived_state);

        // Update Grand Total Stats
        root.stats.total_nodes += 1;
        root.stats.total_cpus += node.cpus as u32;
        root.stats.alloc_cpus += alloc_cpus_for_node;
        if is_available {
            root.stats.idle_nodes += 1;
            root.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
        } else if is_mixed {
            root.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
        }

        if let Some(preempted_nodes_ids) = &preempted_nodes {
            if preempted_nodes_ids.0.contains(&node.id) {
                *root.stats.preempt_nodes.get_or_insert(0) += 1;
            }
        }

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
                } else if is_mixed {
                    current_level.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                }
                if let Some(preempted_nodes_ids) = &preempted_nodes {
                    if preempted_nodes_ids.0.contains(&node.id) {
                        *current_level.stats.preempt_nodes.get_or_insert(0) += 1;
                    }
                }

                if show_node_names {
                    current_level.stats.node_names.push(node.name.clone());
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
                    } else if is_mixed {
                        current_level.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                    }

                    if let Some(preempted_nodes_ids) = &preempted_nodes {
                        if preempted_nodes_ids.0.contains(&node.id) {
                            *current_level.stats.preempt_nodes.get_or_insert(0) += 1;
                        }
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
                        } else if is_mixed {
                            current_level.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                        }

                        if let Some(preempted_nodes_ids) = &preempted_nodes {
                            if preempted_nodes_ids.0.contains(&node.id) {
                                *current_level.stats.preempt_nodes.get_or_insert(0) += 1;
                            }
                        }

                        if show_node_names {
                            current_level.stats.node_names.push(node.name.clone());
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
    max_preempt_nodes_width: usize,
    max_idle_cpus: usize,
    max_total_cpus: usize,
}

fn calculate_column_widths(tree_node: &TreeNode) -> ColumnWidths {
    let mut widths = ColumnWidths {
        max_idle_nodes: tree_node.stats.idle_nodes.to_string().len(),
        max_total_nodes: tree_node.stats.total_nodes.to_string().len(),
        max_preempt_nodes_width: 0, // Start at 0
        max_idle_cpus: tree_node.stats.idle_cpus.to_string().len(),
        max_total_cpus: tree_node.stats.total_cpus.to_string().len(),
    };

    if let Some(count) = tree_node.stats.preempt_nodes {
        widths.max_preempt_nodes_width = count.to_string().len();
    }

    for child in tree_node.children.values() {
        let child_widths = calculate_column_widths(child);
        widths.max_idle_nodes = widths.max_idle_nodes.max(child_widths.max_idle_nodes);
        widths.max_total_nodes = widths.max_total_nodes.max(child_widths.max_total_nodes);
        widths.max_preempt_nodes_width = widths.max_preempt_nodes_width.max(child_widths.max_preempt_nodes_width);
        widths.max_idle_cpus = widths.max_idle_cpus.max(child_widths.max_idle_cpus);
        widths.max_total_cpus = widths.max_total_cpus.max(child_widths.max_total_cpus);
    }

    widths
}

/// Creates a colored bar string for available resources (nodes or CPUs)
fn create_avail_bar(current: u32, total: u32, width: usize, color: Color, no_color: bool) -> String {
    if total == 0 {
        // To avoid division by zero and provide clear output for empty categories
        let bar_content = " ".repeat(width);
        return format!("|{}|", bar_content);
    }

    let percentage = current as f64 / total as f64;

    let bars = count_blocks(20, percentage);
    
    let filled = "█".repeat(bars.0).color(if no_color { Color::White} else { color });
    let empty = " ".repeat(bars.1);

    if let Some(remainder) = bars.2 {
        format!("|{}{}{}|", filled, remainder.color(if no_color { Color::White} else { color }), empty)
    } else {
        format!("|{}{}|", filled, empty)
    }
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

pub fn print_tree_report(root: &TreeReportData, no_color: bool, show_node_names: bool, sort: bool, preempt: bool) {
    // --- Define Headers ---
    const HEADER_FEATURE: &str = "FEATURE (Avail/Total)";
    const HEADER_NODES_PREEMPT: &str = "NODES (PREEMPT)";
    const HEADER_NODES: &str = "NODES";
    const HEADER_CPUS: &str = "CORES";
    const HEADER_NODE_AVAIL: &str = "NODES AVAIL.";
    const HEADER_CPU_AVAIL: &str = "CORES AVAIL.";

    // Calculate Column Widths
    let max_feature_width = calculate_max_width(root, 0).max(HEADER_FEATURE.len()) - 4;
    let bar_width = 20;
    
    let col_widths = calculate_column_widths(root);

    // Calculate data width for the NODES column, accounting for the preempt count string
    let nodes_data_width = {
        let base_width = col_widths.max_idle_nodes + col_widths.max_total_nodes + 1; // for "/"
        if col_widths.max_preempt_nodes_width > 0 {
            // Add width for the preempt count and the "()" characters
            base_width + col_widths.max_preempt_nodes_width + 2
        } else {
            base_width
        }
    };
    
    // CPU column width calculation remains the same
    let cpus_data_width = col_widths.max_idle_cpus + col_widths.max_total_cpus + 1;

    // The final column width is the larger of the header or the data
    let nodes_final_width =  if preempt {
        nodes_data_width.max(HEADER_NODES_PREEMPT.len())
    } else {
        nodes_data_width.max(HEADER_NODES.len())
    };

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
    
    let stats = &top_level_node.stats;

    let (node_text, uncolored_node_text) = {
        let idle_str = format!("{:>width$}", stats.idle_nodes, width = col_widths.max_idle_nodes);
        let total_str = format!("{:>width$}", stats.total_nodes, width = col_widths.max_total_nodes);

        if let Some(preempt_count) = stats.preempt_nodes {
            let preempt_str_colored = format!("({:>width$})", preempt_count, width = col_widths.max_preempt_nodes_width).yellow().to_string();
            let preempt_str_uncolored = format!("({:>width$})", preempt_count, width = col_widths.max_preempt_nodes_width);
            (
                format!("{}{}/{}", idle_str, preempt_str_colored, total_str),
                format!("{}{}/{}", idle_str, preempt_str_uncolored, total_str)
            )
        } else {
            let text = if col_widths.max_preempt_nodes_width > 0 {
                let padding = " ".repeat(col_widths.max_preempt_nodes_width + 2);
                format!("{}{}/{}", idle_str, padding, total_str)
            } else {
                format!("{}/{}", idle_str, total_str)
            };
            (text.clone(), text)
        }
    };
    let nodes_width_adjusted = nodes_final_width + node_text.len() - uncolored_node_text.len();

    let cpu_text = format!(
        "{:>idle_w$}/{:>total_w$}",
        stats.idle_cpus,
        stats.total_cpus,
        idle_w = col_widths.max_idle_cpus,
        total_w = col_widths.max_total_cpus
    );
    let cpus_width_adjusted = cpus_final_width;

    let node_bar = create_avail_bar(stats.idle_nodes, stats.total_nodes, bar_width, Color::Green, no_color);
    let cpu_bar = create_avail_bar(stats.idle_cpus, stats.total_cpus, bar_width, Color::Cyan, no_color);

    // Print Headers with left-alignment
    println!(
        "{:<feature_w$} {:<nodes_w$}  {:<bar_w$}{:<cpus_w$}  {:<bar_w$}",
        HEADER_FEATURE.bold(),
        if preempt {HEADER_NODES_PREEMPT.bold()} else {HEADER_NODES.bold()},
        HEADER_NODE_AVAIL.bold(),
        HEADER_CPUS.bold(),
        HEADER_CPU_AVAIL.bold(),
        feature_w = max_feature_width,
        nodes_w = nodes_final_width,
        cpus_w = cpus_final_width,
        bar_w = bar_final_width
    );

    // Print Separator Line
    let total_width = max_feature_width + nodes_final_width + cpus_final_width + bar_final_width * 2 + 6; // +6 for spaces
    println!("{}", "-".repeat(total_width - 2));

    // Print the top-level line using the adjusted widths for proper alignment
    println!(
        "{:<feature_w$} {:>nodes_w$} {} {:>cpus_w$} {}",
        top_level_node.name.bold(),
        node_text,
        node_bar,
        cpu_text,
        cpu_bar,
        feature_w = max_feature_width,
        nodes_w = nodes_width_adjusted,
        cpus_w = cpus_width_adjusted
    );

    // Print the children recursively
    let mut sorted_children: Vec<_> = children_to_iterate.values().collect();
    if !sort {
        sorted_children.sort_by(|a, b| b.stats.total_nodes.cmp(&a.stats.total_nodes));
    } else {
        sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
    }
    for (i, child) in sorted_children.iter().enumerate() {
        let is_last = i == sorted_children.len() - 1;
        print_node_recursive(
            child,
            "",
            is_last,
            no_color,
            (max_feature_width, bar_width, nodes_final_width, cpus_final_width),
            &col_widths,
            show_node_names,
            sort,
        );
    }
}

/// Recursively prints a node and its children to form the tree structure
#[allow(clippy::too_many_arguments)]
fn print_node_recursive(
    tree_node: &TreeNode,
    prefix: &str,
    is_last: bool,
    no_color: bool,
    widths: (usize, usize, usize, usize),
    col_widths: &ColumnWidths,
    show_node_names: bool,
    sort: bool,
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
    
    let stats = &current_node.stats;

    let (node_text, uncolored_node_text) = {
        let idle_str = format!("{:>width$}", stats.idle_nodes, width = col_widths.max_idle_nodes);
        let total_str = format!("{:>width$}", stats.total_nodes, width = col_widths.max_total_nodes);

        if let Some(preempt_count) = stats.preempt_nodes {
            let preempt_str_colored = format!("({:>width$})", preempt_count, width = col_widths.max_preempt_nodes_width).yellow().to_string();
            let preempt_str_uncolored = format!("({:>width$})", preempt_count, width = col_widths.max_preempt_nodes_width);
            (
                format!("{}{}/{}", idle_str, preempt_str_colored, total_str),
                format!("{}{}/{}", idle_str, preempt_str_uncolored, total_str)
            )
        } else {
            let text = if col_widths.max_preempt_nodes_width > 0 {
                let padding = " ".repeat(col_widths.max_preempt_nodes_width + 2);
                format!("{}{}/{}", idle_str, padding, total_str)
            } else {
                format!("{}/{}", idle_str, total_str)
            };
            (text.clone(), text)
        }
    };
    let nodes_width_adjusted = nodes_final_width + node_text.len() - uncolored_node_text.len();

    let cpu_text = format!(
        "{:>idle_w$}/{:>total_w$}",
        stats.idle_cpus,
        stats.total_cpus,
        idle_w = col_widths.max_idle_cpus,
        total_w = col_widths.max_total_cpus
    );
    let cpus_width_adjusted = cpus_final_width;

    let node_bar = create_avail_bar(stats.idle_nodes, stats.total_nodes, bar_width, Color::Green, no_color);
    let cpu_bar = create_avail_bar(stats.idle_cpus, stats.total_cpus, bar_width, Color::Cyan, no_color);
    let node_names = &current_node.stats.node_names.clone();

    println!(
        "{:<feature_w$} {:>nodes_w$} {} {:>cpus_w$} {} {}",
        display_name.bold(),
        node_text,
        node_bar,
        cpu_text,
        cpu_bar,
        if show_node_names {fi_slurm::parser::compress_hostlist(node_names)} else {"".to_string()},
        feature_w = max_width,
        nodes_w = nodes_width_adjusted,
        cpus_w = cpus_width_adjusted,
    );

    let full_child_prefix = format!("{}{}", prefix, if is_last { "   " } else { "│  " });
    let mut sorted_children: Vec<_> = current_node.children.values().collect();
    if !sort {
        sorted_children.sort_by(|a, b| b.stats.total_nodes.cmp(&a.stats.total_nodes));
    } else {
        sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
    }

    for (i, child) in sorted_children.iter().enumerate() {
        let is_child_last = i == sorted_children.len() - 1;
        print_node_recursive(
            child,
            &full_child_prefix,
            is_child_last,
            no_color,
            (max_width, bar_width, nodes_final_width, cpus_final_width),
            col_widths,
            show_node_names,
            sort,
        );
    }
}


