use crate::nodes::{Node, NodeState, SlurmNodes};
use crate::jobs::SlurmJobs; // Needed for CPU allocation calculation
use colored::*;
use std::collections::HashMap;

// Data Structures 

/// Represents a single node in the feature hierarchy tree
/// This struct is recursive, as its `children` field contains more `TreeNodes`
#[derive(Default, Debug, Clone)]
pub struct TreeNode {
    /// The name of this feature (e.g., "gpu", "icelake")
    pub name: String,
    
    /// Aggregated statistics for all nodes that fall under this branch of the tree
    pub stats: ReportLine,

    /// A map of child features, allowing the tree to branch
    /// e.g., an "icelake" node might have children for different GRES types
    pub children: HashMap<String, TreeNode>,
}

/// A simplified version of the ReportLine from the detailed report
#[derive(Default, Debug, Clone)]
pub struct ReportLine {
    pub node_count: u32,
    pub total_cpus: u32,
    pub alloc_cpus: u32,
    // We can add GPU stats here later if needed
}

// The root of our report is just a TreeNode, often with a name like "root" or "total"
pub type TreeReportData = TreeNode;


// Aggregation Logic

/// Builds a hierarchical tree report from a flat list of Slurm nodes
///
/// This function iterates through all nodes and constructs a tree based on their
/// ordered list of features
///
/// # Arguments
/// * `nodes` - A reference to the list of nodes to include in the report
/// * `jobs` - A reference to the fully loaded `SlurmJobs` collection
/// * `node_to_job_map` - A map from node names to the jobs running on them
///
/// # Returns
/// A `TreeReportData` (which is a `TreeNode`) representing the root of the report tree
pub fn build_tree_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<String, Vec<u32>>,
) -> TreeReportData {
    // The root of our tree doesn't represent a real feature, just the grand total
    let mut root = TreeNode {
        name: "TOTAL".to_string(),
        ..Default::default()
    };

    for node in nodes {
        // Calculate Allocated CPUs for this specific node
        let alloc_cpus_for_node: u32 = if let Some(job_ids) = node_to_job_map.get(&node.name) {
            job_ids.iter().filter_map(|id| jobs.jobs.get(id)).map(|j| j.num_cpus / j.num_nodes.max(1)).sum()
        } else {
            0
        };

        // For each node, we "walk" down the tree, creating branches as needed
        let mut current_level = &mut root;

        // Add the node's stats to the root total
        current_level.stats.node_count += 1;
        current_level.stats.total_cpus += node.cpus as u32;
        current_level.stats.alloc_cpus += alloc_cpus_for_node;
        
        // The path for this node is its list of features
        for feature in &node.features {
            // Move down one level in the tree
            current_level = current_level.children.entry(feature.clone()).or_default();
            current_level.name = feature.clone(); // Ensure the name is set on creation

            // Add the node's stats to this feature's branch as well
            current_level.stats.node_count += 1;
            current_level.stats.total_cpus += node.cpus as u32;
            current_level.stats.alloc_cpus += alloc_cpus_for_node;
        }
    }
    root
}

// Display Logic

/// The public entry point for printing the tree report
pub fn print_tree_report(root: &TreeReportData) {
    println!("--- Feature Tree Report ---");
    // Kick off the recursive printing process starting at the root's children
    let mut sorted_children: Vec<_> = root.children.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));

    for (i, child) in sorted_children.iter().enumerate() {
        let is_last = i == sorted_children.len() - 1;
        print_node_recursive(child, "", is_last);
    }
}

/// Recursively prints a node and its children to form the tree structure
fn print_node_recursive(tree_node: &TreeNode, prefix: &str, is_last: bool) {
    // Determine the correct prefix for the current node
    // This creates the `|--` and ` `--` tree branches
    let connector = if is_last { "└──" } else { "├──" };
    println!("{}{}{}", prefix, connector, tree_node.name.bold());

    // Create the prefix for this node's children
    // Children get a deeper indentation
    let child_prefix = if is_last { "    " } else { "│   " };
    let full_child_prefix = format!("{}{}", prefix, child_prefix);

    // Sort and print children recursively
    let mut sorted_children: Vec<_> = tree_node.children.values().collect();
    sorted_children.sort_by(|a, b| a.name.cmp(&b.name));
    
    for (i, child) in sorted_children.iter().enumerate() {
        let is_child_last = i == sorted_children.len() - 1;
        print_node_recursive(child, &full_child_prefix, is_child_last);
    }
}
