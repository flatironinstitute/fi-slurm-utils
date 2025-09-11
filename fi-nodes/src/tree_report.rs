use crate::PreemptNodes;
use colored::*;
use fi_slurm::jobs::SlurmJobs;
use fi_slurm::nodes::{Node, NodeState};
use fi_slurm::utils::count_blocks;
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

// a custom list of uninformative or redundant features excluded from the default presentation
static HIDDEN_FEATURES: OnceLock<HashSet<&str>> = OnceLock::new();

// TODO: per-site hidden feature configuration
fn hidden_features() -> &'static HashSet<&'static str> {
    HIDDEN_FEATURES.get_or_init(|| {
        [
            "rocky8", "rocky9", "sxm", "sxm2", "sxm4", "sxm5", "nvlink", "a100", "h100", "v100", "ib",
        ]
        .iter()
        .cloned()
        .collect()
    })
}

// Data Structures for the Tree Report

/// Represents a single node in the feature hierarchy tree
#[derive(Default, Debug, Clone)]
pub struct TreeNode {
    pub name: String,
    pub stats: ReportLine,
    pub single_filter: bool, // used to determine whether we are filtering on a single item
    pub children: HashMap<String, TreeNode>,
}

/// A simplified version of the ReportLine from the detailed report
#[derive(Default, Debug, Clone)]
pub struct ReportLine {
    pub total_nodes: u32,
    pub idle_nodes: u32,
    pub preempt_nodes: Option<u32>,
    pub total_cpus: u32,
    pub idle_cpus: u32,
    pub preempt_cpus: Option<u32>,
    pub alloc_cpus: u32,
    pub node_names: Vec<String>,
}

/// A Newtype for TreeNode, representing the output of build_tree_report
pub type TreeReportData = TreeNode;

// Aggregation Logic

/// Helper function to determine if a node is available for new work
fn is_node_available(state: &NodeState) -> bool {
    match state {
        NodeState::Idle => true,
        NodeState::Compound { base, flags } => {
            if **base == NodeState::Idle {
                // Node is idle, but check for disqualifying flags
                !flags.iter().any(|flag| {
                    flag == "MAINT" || flag == "DOWN" || flag == "DRAIN" || flag == "INVALID_REG"
                })
            } else {
                false
            }
        }
        _ => false,
    }
}

/// Helper function to determine if a node partly available for new work
fn is_node_mixed(state: &NodeState) -> bool {
    match state {
        NodeState::Mixed => true,
        NodeState::Compound { base, flags } => {
            if **base == NodeState::Mixed {
                // Node is mixed, but check for disqualifying flags
                !flags.iter().any(|flag| {
                    flag == "MAINT" || flag == "DOWN" || flag == "DRAIN" || flag == "INVALID_REG"
                })
            } else {
                false
            }
        }
        _ => false,
    }
}

/// A filter enum to decide whether we want to show only nodes with gpu, nodes without gpu, or show both
pub enum GpuFilter {
    Gpu,
    NotGpu,
    All,
}

/// Builds a hierarchical tree report from a flat list of Slurm nodes
/// Strong candidate for refactor, currently very repetitive and confusing
#[allow(clippy::too_many_arguments)]
pub fn build_tree_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<usize, Vec<u32>>,
    feature_filter: &[String],
    show_hidden_features: bool,
    show_node_names: bool,
    preempted_nodes: Option<PreemptNodes>,
    preempt: bool,
    gpu: bool,
) -> TreeReportData {
    let mut root = TreeNode {
        name: "Total".to_string(),
        ..Default::default()
    };

    if feature_filter.len() == 1 {
        root.single_filter = true
    };

    // the main loop, iterating over the nodes in order to construct the tree structure
    for &node in nodes {
        let alloc_cpus_for_node: u32 = if let Some(job_ids) = node_to_job_map.get(&node.id) {
            job_ids
                .iter()
                .filter_map(|id| jobs.jobs.get(id))
                .map(|j| j.num_cpus / j.num_nodes.max(1))
                .sum()
        } else {
            0
        };

        let mut total_gpus: u32 = 0;
        let mut allocated_gpus: u32 = 0;

        if let Some(gpu_info) = &node.gpu_info {
            total_gpus = gpu_info.total_gpus as u32;
            allocated_gpus = gpu_info.allocated_gpus as u32;
        };

        let derived_state = if alloc_cpus_for_node > 0 && alloc_cpus_for_node < node.cpus as u32 {
            match &node.state {
                NodeState::Compound { flags, .. } => NodeState::Compound {
                    base: Box::new(NodeState::Mixed),
                    flags: flags.to_vec(),
                },
                _ => NodeState::Mixed,
            }
        } else {
            // Otherwise, we trust the state reported by Slurm
            node.state.clone()
        };

        let is_available = is_node_available(&derived_state);
        let is_mixed = is_node_mixed(&derived_state);

        let preempted_node_ids = if preempt {
            &preempted_nodes.as_ref().unwrap().0
        } else {
            &Vec::new()
        };

        // Update Grand Total Stats
        root.stats.total_nodes += 1;

        if gpu {
            root.stats.total_cpus += total_gpus;
            root.stats.alloc_cpus += allocated_gpus;
        } else {
            root.stats.total_cpus += node.cpus as u32;
            root.stats.alloc_cpus += alloc_cpus_for_node;
        }

        if is_available && preempt {
            // we don't increment idle nodes or cpus in this case in this case
            // in order to keep idle nodes referring only to idle and not idle + preempt
            root.stats.idle_nodes += 1;

            if gpu {
                root.stats.idle_cpus = total_gpus;
            } else {
                root.stats.idle_cpus += node.cpus as u32;
            }

            if preempted_node_ids.contains(&node.id) {
                *root.stats.preempt_nodes.get_or_insert(0) += 1;
                if gpu {
                    *root.stats.preempt_cpus.get_or_insert(0) += total_gpus;
                } else {
                    *root.stats.preempt_cpus.get_or_insert(0) += node.cpus as u32; // because unlike
                }
                // the nodes, cpus don't get any kind of base state change
            }
        } else if is_available && !preempt {
            root.stats.idle_nodes += 1;
            // we assume that, if we're using the gpu bool flag and have gotten to this point, all
            // the nodes we loop over will unwrap without panicking, since is_some was the
            // inclusion condition
            // but the mixed logic above may not be fully accurate...
            if gpu {
                root.stats.idle_cpus += (total_gpus).saturating_sub(allocated_gpus);
            } else {
                root.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
            }
        } else if is_mixed && preempt {
            if gpu {
                root.stats.idle_cpus += (total_gpus).saturating_sub(allocated_gpus);
            } else {
                root.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
            }

            if preempted_node_ids.contains(&node.id) {
                if gpu {
                    *root.stats.preempt_cpus.get_or_insert(0) +=
                        (total_gpus).saturating_sub(allocated_gpus);
                } else {
                    *root.stats.preempt_cpus.get_or_insert(0) +=
                        (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                }
            }
        } else if is_mixed && !preempt {
            if gpu {
                root.stats.idle_cpus += (total_gpus).saturating_sub(allocated_gpus);
            } else {
                root.stats.idle_cpus += (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
            }
        }

        // we filter the features list to remove the undesired features unless told otherwise
        let features_for_tree: Vec<_> = if show_hidden_features {
            node.features.iter().collect()
        } else {
            node.features
                .iter()
                .filter(|f| !hidden_features().contains(f.as_str()))
                .collect()
        };

        // further refine with either gpu, not gpu, or both

        // tree building logic
        if feature_filter.is_empty() {
            // by default, build tree from the (potentially filtered) feature list
            let mut current_level = &mut root;
            for feature in &features_for_tree {
                current_level = current_level
                    .children
                    .entry(feature.to_string())
                    .or_default();
                current_level.name = feature.to_string();
                // add stats to this branch
                current_level.stats.total_nodes += 1;

                if gpu {
                    current_level.stats.total_cpus += total_gpus;
                    current_level.stats.alloc_cpus += allocated_gpus;
                } else {
                    current_level.stats.total_cpus += node.cpus as u32;
                    current_level.stats.alloc_cpus += alloc_cpus_for_node;
                }

                if is_available && preempt {
                    current_level.stats.idle_nodes += 1;

                    if gpu {
                        current_level.stats.idle_cpus += total_gpus;
                    } else {
                        current_level.stats.idle_cpus += node.cpus as u32;
                    }

                    if preempted_node_ids.contains(&node.id) {
                        *current_level.stats.preempt_nodes.get_or_insert(0) += 1;
                        if gpu {
                            *current_level.stats.preempt_cpus.get_or_insert(0) += total_gpus;
                        } else {
                            *current_level.stats.preempt_cpus.get_or_insert(0) += node.cpus as u32;
                        }
                    }
                } else if is_available && !preempt {
                    current_level.stats.idle_nodes += 1;
                    if gpu {
                        current_level.stats.idle_cpus +=
                            (total_gpus).saturating_sub(allocated_gpus);
                    } else {
                        current_level.stats.idle_cpus +=
                            (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                    }
                } else if is_mixed && preempt {
                    if gpu {
                        current_level.stats.idle_cpus +=
                            (total_gpus).saturating_sub(allocated_gpus);
                    } else {
                        current_level.stats.idle_cpus +=
                            (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                    }

                    if preempted_node_ids.contains(&node.id) {
                        if gpu {
                            *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                (total_gpus).saturating_sub(allocated_gpus);
                        } else {
                            *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                        }
                    }
                } else if is_mixed && !preempt {
                    if gpu {
                        current_level.stats.idle_cpus +=
                            (total_gpus).saturating_sub(allocated_gpus);
                    } else {
                        current_level.stats.idle_cpus +=
                            (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                    }
                }

                if show_node_names {
                    current_level.stats.node_names.push(node.name.clone());
                }
            }
        } else {
            // bring the filtered features to the top level
            for filter in feature_filter {
                // IMPORTANT: The check to see if a node belongs under a filter
                // must use the ORIGINAL, unfiltered features.
                if node.features.contains(filter) {
                    let mut current_level = root.children.entry(filter.clone()).or_default();
                    current_level.name = filter.clone();
                    // add stats to this top-level branch
                    current_level.stats.total_nodes += 1;

                    if gpu {
                        current_level.stats.total_cpus += total_gpus;
                        current_level.stats.alloc_cpus += allocated_gpus;
                    } else {
                        current_level.stats.total_cpus += node.cpus as u32;
                        current_level.stats.alloc_cpus += alloc_cpus_for_node;
                    }

                    if is_available && preempt {
                        current_level.stats.idle_nodes += 1;
                        if gpu {
                            current_level.stats.idle_cpus += total_gpus;
                        } else {
                            current_level.stats.idle_cpus += node.cpus as u32;
                        }

                        if preempted_node_ids.contains(&node.id) {
                            *current_level.stats.preempt_nodes.get_or_insert(0) += 1;

                            if gpu {
                                *current_level.stats.preempt_cpus.get_or_insert(0) += total_gpus;
                            } else {
                                *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                    node.cpus as u32;
                            }
                        }
                    } else if is_available && !preempt {
                        current_level.stats.idle_nodes += 1;
                        if gpu {
                            current_level.stats.idle_cpus +=
                                (total_gpus).saturating_sub(allocated_gpus);
                        } else {
                            current_level.stats.idle_cpus +=
                                (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                        }
                    } else if is_mixed && preempt {
                        if gpu {
                            current_level.stats.idle_cpus +=
                                (total_gpus).saturating_sub(allocated_gpus);
                        } else {
                            current_level.stats.idle_cpus +=
                                (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                        }
                        if preempted_node_ids.contains(&node.id) {
                            if gpu {
                                *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                    (total_gpus).saturating_sub(allocated_gpus);
                            } else {
                                *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                    (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                            }
                        }
                    } else if is_mixed && !preempt {
                        if gpu {
                            current_level.stats.idle_cpus +=
                                (total_gpus).saturating_sub(allocated_gpus);
                        } else {
                            current_level.stats.idle_cpus +=
                                (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                        }
                    }

                    // build the sub-branch from the *remaining* features,
                    // respecting the show_hidden_features flag
                    for feature in features_for_tree.iter().filter(|f| f.as_str() != filter) {
                        current_level = current_level
                            .children
                            .entry(feature.to_string())
                            .or_default();
                        current_level.name = feature.to_string();
                        // add stats to the sub-branch
                        current_level.stats.total_nodes += 1;

                        if gpu {
                            current_level.stats.total_cpus += total_gpus;
                            current_level.stats.alloc_cpus += allocated_gpus;
                        } else {
                            current_level.stats.total_cpus += node.cpus as u32;
                            current_level.stats.alloc_cpus += alloc_cpus_for_node;
                        }

                        if is_available && preempt {
                            current_level.stats.idle_nodes += 1;

                            if gpu {
                                current_level.stats.idle_cpus += total_gpus;
                            } else {
                                current_level.stats.idle_cpus += node.cpus as u32;
                            }

                            if preempted_node_ids.contains(&node.id) {
                                *current_level.stats.preempt_nodes.get_or_insert(0) += 1;

                                if gpu {
                                    *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                        total_gpus;
                                } else {
                                    *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                        node.cpus as u32;
                                }
                            }
                        } else if is_available && !preempt {
                            current_level.stats.idle_nodes += 1;
                            if gpu {
                                current_level.stats.idle_cpus +=
                                    (total_gpus).saturating_sub(allocated_gpus);
                            } else {
                                current_level.stats.idle_cpus +=
                                    (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                            }
                        } else if is_mixed && preempt {
                            if gpu {
                                current_level.stats.idle_cpus +=
                                    (total_gpus).saturating_sub(allocated_gpus);
                            } else {
                                current_level.stats.idle_cpus +=
                                    (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                            }

                            if preempted_node_ids.contains(&node.id) {
                                if gpu {
                                    *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                        (total_gpus).saturating_sub(allocated_gpus);
                                } else {
                                    *current_level.stats.preempt_cpus.get_or_insert(0) +=
                                        (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
                                }
                            }
                        } else if is_mixed && !preempt {
                            if gpu {
                                current_level.stats.idle_cpus +=
                                    (total_gpus).saturating_sub(allocated_gpus);
                            } else {
                                current_level.stats.idle_cpus +=
                                    (node.cpus as u32).saturating_sub(alloc_cpus_for_node);
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

/// Struct containing the widths of each column
#[derive(Default)]
struct ColumnWidths {
    max_idle_nodes: usize,
    max_total_nodes: usize,
    max_preempt_nodes_width: usize,
    max_idle_cpus: usize,
    max_total_cpus: usize,
    max_preempt_cpus_width: usize,
}

/// Helper function for calculating the widths of the columns
fn calculate_column_widths(tree_node: &TreeNode) -> ColumnWidths {
    let mut widths = ColumnWidths {
        max_idle_nodes: tree_node.stats.idle_nodes.to_string().len(),
        max_total_nodes: tree_node.stats.total_nodes.to_string().len(),
        max_preempt_nodes_width: 0, // Start at 0
        max_idle_cpus: tree_node.stats.idle_cpus.to_string().len(),
        max_total_cpus: tree_node.stats.total_cpus.to_string().len(),
        max_preempt_cpus_width: 0,
    };

    if let Some(node_count) = tree_node.stats.preempt_nodes {
        widths.max_preempt_nodes_width = node_count.to_string().len();
    }
    if let Some(cpu_count) = tree_node.stats.preempt_cpus {
        widths.max_preempt_cpus_width = cpu_count.to_string().len();
    }

    for child in tree_node.children.values() {
        let child_widths = calculate_column_widths(child);
        widths.max_idle_nodes = widths.max_idle_nodes.max(child_widths.max_idle_nodes);
        widths.max_total_nodes = widths.max_total_nodes.max(child_widths.max_total_nodes);
        widths.max_preempt_nodes_width = widths
            .max_preempt_nodes_width
            .max(child_widths.max_preempt_nodes_width);
        widths.max_idle_cpus = widths.max_idle_cpus.max(child_widths.max_idle_cpus);
        widths.max_total_cpus = widths.max_total_cpus.max(child_widths.max_total_cpus);
        widths.max_preempt_cpus_width = widths
            .max_preempt_cpus_width
            .max(child_widths.max_preempt_cpus_width);
    }

    widths
}

/// Creates a colored bar string for available resources (nodes or CPUs)
fn create_avail_bar(
    current: u32,
    total: u32,
    width: usize,
    color: Color,
    no_color: bool,
) -> String {
    if total == 0 {
        // To avoid division by zero and provide clear output for empty categories
        let bar_content = " ".repeat(width);
        return format!("│{}│", bar_content);
    }

    let percentage = current as f64 / total as f64;

    let bars = count_blocks(20, percentage);

    let filled = "█"
        .repeat(bars.0)
        .color(if no_color { Color::White } else { color });
    let empty = " ".repeat(bars.1);

    if let Some(remainder) = bars.2 {
        format!(
            "│{}{}{}│",
            filled,
            remainder.color(if no_color { Color::White } else { color }),
            empty
        )
    } else {
        format!("│{}{}│", filled, empty)
    }
}

/// Recursively calculates the maximum width needed for the feature name column
fn calculate_max_width(tree_node: &TreeNode, prefix_len: usize, collapse: bool) -> usize {
    let mut path_parts = vec![tree_node.name.as_str()];
    let mut current_node = tree_node;
    if collapse {
        while current_node.children.len() == 1 {
            let single_child = current_node.children.values().next().unwrap();
            path_parts.push(single_child.name.as_str());
            current_node = single_child;
        }
    }
    let collapsed_name = path_parts.join(", ");
    let current_width = prefix_len + collapsed_name.len() + 5; // +3 for "└──", +2 for visual padding

    current_node
        .children
        .values()
        .map(|child| calculate_max_width(child, prefix_len + 3, true))
        .max()
        .unwrap_or(0)
        .max(current_width)
}

/// Prints the tree report
pub fn print_tree_report(
    root: &TreeReportData,
    no_color: bool,
    show_node_names: bool,
    sort: bool,
    preempt: bool,
    gpu: bool,
) {
    // --- Define Headers ---
    const HEADER_FEATURE: &str = "Feature";
    const HEADER_NODES_PREEMPT: &str = "";
    const HEADER_NODES: &str = "";
    const HEADER_CPUS_PREEMPT: &str = "";
    const HEADER_CPUS: &str = "";
    const HEADER_GPUS_PREEMPT: &str = "";
    const HEADER_GPUS: &str = "";
    const HEADER_NODE_AVAIL: &str = "Nodes Available  ";
    const HEADER_CPU_AVAIL: &str = "Cores Available  ";
    const HEADER_GPU_AVAIL: &str = "GPUs Available  ";

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

    // Calculate Column Widths
    let max_feature_width =
        calculate_max_width(top_level_node, 0, false).max(HEADER_FEATURE.len()) - 4;
    let bar_width = 20;

    let col_widths = calculate_column_widths(top_level_node);

    // Calculate data width for the NODES column, accounting for the preempt count string
    let nodes_data_width = {
        let base_width = col_widths.max_idle_nodes + col_widths.max_total_nodes + 1; // for "/"
        if col_widths.max_preempt_nodes_width > 0 {
            // Add width for the preempt count and the "(-)" characters
            base_width + col_widths.max_preempt_nodes_width + 3
        } else {
            base_width
        }
    };

    // CPU column width calculation, accounting for preempt count string if needed
    // let cpus_data_width = col_widths.max_idle_cpus + col_widths.max_total_cpus + 1;
    let cpus_data_width = {
        let base_width = col_widths.max_idle_cpus + col_widths.max_total_cpus + 1; // for "/"
        if col_widths.max_preempt_cpus_width > 0 {
            // Add width for the preempt count and the "(-)" characters
            base_width + col_widths.max_preempt_cpus_width + 3
        } else {
            base_width
        }
    };

    // The final column width is the larger of the header or the data
    let nodes_final_width = if preempt {
        nodes_data_width.max(HEADER_NODES_PREEMPT.len())
    } else {
        nodes_data_width.max(HEADER_NODES.len())
    };

    let cpus_final_width = if preempt {
        if gpu {
            (cpus_data_width).max(HEADER_GPUS_PREEMPT.len())
        } else {
            (cpus_data_width).max(HEADER_CPUS_PREEMPT.len())
        }
    } else if gpu {
        (cpus_data_width).max(HEADER_GPUS.len())
    } else {
        (cpus_data_width).max(HEADER_CPUS.len())
    };
    let bar_final_width = (bar_width + 2).max(HEADER_NODE_AVAIL.len()); // +2 for "||"

    let stats = &top_level_node.stats;

    let (node_text, uncolored_node_text) = {
        let idle_str = format!(
            "{:>width$}",
            stats.idle_nodes,
            width = col_widths.max_idle_nodes
        );
        let total_str = format!(
            "{:>width$}",
            stats.total_nodes,
            width = col_widths.max_total_nodes
        );

        if let Some(preempt_count) = stats.preempt_nodes {
            let preempt_str_colored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_nodes_width
            )
            .yellow()
            .to_string();
            let preempt_str_uncolored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_nodes_width
            );
            (
                format!("{}{}/{}", idle_str, preempt_str_colored, total_str),
                format!("{}{}/{}", idle_str, preempt_str_uncolored, total_str),
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

    let (cpu_text, uncolored_cpu_text) = {
        let idle_str = format!(
            "{:>width$}",
            stats.idle_cpus,
            width = col_widths.max_idle_cpus
        );
        let total_str = format!(
            "{:>width$}",
            stats.total_cpus,
            width = col_widths.max_total_cpus
        );

        if let Some(preempt_count) = stats.preempt_cpus {
            let preempt_str_colored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_cpus_width
            )
            .yellow()
            .to_string();
            let preempt_str_uncolored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_cpus_width
            );
            (
                format!("{}{}/{}", idle_str, preempt_str_colored, total_str),
                format!("{}{}/{}", idle_str, preempt_str_uncolored, total_str),
            )
        } else {
            let text = if col_widths.max_preempt_cpus_width > 0 {
                let padding = " ".repeat(col_widths.max_preempt_cpus_width + 3);
                format!("{}{}/{}", idle_str, padding, total_str)
            } else {
                format!("{}/{}", idle_str, total_str)
            };
            (text.clone(), text)
        }
    };
    let cpus_width_adjusted = cpus_final_width + cpu_text.len() - uncolored_cpu_text.len();

    // getting the true max at the top level

    let max_nodes = stats.total_nodes;
    let max_cores = stats.total_cpus;

    let node_bar = create_avail_bar(
        stats.idle_nodes,
        stats.total_nodes,
        bar_width,
        Color::Green,
        no_color,
    );
    let cpu_bar = if gpu {
        create_avail_bar(
            stats.idle_cpus,
            stats.total_cpus,
            bar_width,
            Color::Red,
            no_color,
        )
    } else {
        create_avail_bar(
            stats.idle_cpus,
            stats.total_cpus,
            bar_width,
            Color::Cyan,
            no_color,
        )
    };

    // Print Headers with alignment
    println!(
        "{:<feature_w$} {:<nodes_w$}  {:<bar_w$}{:<cpus_w$}  {:<bar_w$}",
        HEADER_FEATURE.bold(),
        if preempt {
            HEADER_NODES_PREEMPT.bold()
        } else {
            HEADER_NODES.bold()
        },
        HEADER_NODE_AVAIL.bold(),
        if preempt {
            if gpu {
                HEADER_GPUS_PREEMPT.bold()
            } else {
                HEADER_CPUS_PREEMPT.bold()
            }
        } else if gpu {
            HEADER_GPUS.bold()
        } else {
            HEADER_CPUS.bold()
        },
        if gpu {
            HEADER_GPU_AVAIL.bold()
        } else {
            HEADER_CPU_AVAIL.bold()
        },
        feature_w = max_feature_width,
        nodes_w = nodes_final_width,
        cpus_w = cpus_final_width,
        bar_w = bar_final_width
    );

    // Print Separator Line
    let total_width =
        max_feature_width + nodes_final_width + cpus_final_width + bar_final_width * 2 + 6; // +6 for spaces
    println!("{}", "═".repeat(total_width - 2));

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
            (
                max_feature_width,
                bar_width,
                nodes_final_width,
                cpus_final_width,
            ),
            &col_widths,
            show_node_names,
            sort,
            (max_nodes, max_cores),
            gpu,
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
    max: (u32, u32),
    gpu: bool,
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
        let idle_str = format!(
            "{:>width$}",
            stats.idle_nodes,
            width = col_widths.max_idle_nodes
        );
        let total_str = format!(
            "{:>width$}",
            stats.total_nodes,
            width = col_widths.max_total_nodes
        );

        if let Some(preempt_count) = stats.preempt_nodes {
            let preempt_str_colored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_nodes_width
            )
            .yellow()
            .to_string();
            let preempt_str_uncolored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_nodes_width
            );
            (
                format!("{}{}/{}", idle_str, preempt_str_colored, total_str),
                format!("{}{}/{}", idle_str, preempt_str_uncolored, total_str),
            )
        } else {
            let text = if col_widths.max_preempt_nodes_width > 0 {
                let padding = " ".repeat(col_widths.max_preempt_nodes_width + 3);
                format!("{}{}/{}", idle_str, padding, total_str)
            } else {
                format!("{}/{}", idle_str, total_str)
            };
            (text.clone(), text)
        }
    };
    let nodes_width_adjusted = nodes_final_width + node_text.len() - uncolored_node_text.len();

    let (cpu_text, uncolored_cpu_text) = {
        let idle_str = format!(
            "{:>width$}",
            stats.idle_cpus,
            width = col_widths.max_idle_cpus
        );
        let total_str = format!(
            "{:>width$}",
            stats.total_cpus,
            width = col_widths.max_total_cpus
        );

        if let Some(preempt_count) = stats.preempt_cpus {
            let preempt_str_colored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_cpus_width
            )
            .yellow()
            .to_string();
            let preempt_str_uncolored = format!(
                "(-{:>width$})",
                preempt_count,
                width = col_widths.max_preempt_cpus_width
            );
            (
                format!("{}{}/{}", idle_str, preempt_str_colored, total_str),
                format!("{}{}/{}", idle_str, preempt_str_uncolored, total_str),
            )
        } else {
            let text = if col_widths.max_preempt_cpus_width > 0 {
                let padding = " ".repeat(col_widths.max_preempt_cpus_width + 3);
                format!("{}{}/{}", idle_str, padding, total_str)
            } else {
                format!("{}/{}", idle_str, total_str)
            };
            (text.clone(), text)
        }
    };
    let cpus_width_adjusted = cpus_final_width + cpu_text.len() - uncolored_cpu_text.len();

    let node_bar = create_avail_bar(stats.idle_nodes, max.0, bar_width, Color::Green, no_color);

    let cpu_bar = if gpu {
        create_avail_bar(stats.idle_cpus, max.1, bar_width, Color::Red, no_color)
    } else {
        create_avail_bar(stats.idle_cpus, max.1, bar_width, Color::Cyan, no_color)
    };

    let node_names = &current_node.stats.node_names.clone();

    println!(
        "{:<feature_w$} {:>nodes_w$} {} {:>cpus_w$} {} {}",
        display_name.bold(),
        node_text,
        node_bar,
        cpu_text,
        cpu_bar,
        if show_node_names {
            fi_slurm::parser::compress_hostlist(node_names)
        } else {
            "".to_string()
        },
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
            (max.0, max.1),
            gpu,
        );
    }
}
