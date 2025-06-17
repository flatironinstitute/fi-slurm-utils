use crate::nodes::{NodeState, SlurmNodes};
use std::collections::HashMap;

/// Holds the aggregated statistics for a single feature across the entire cluster.
#[derive(Default, Debug, Clone)]
pub struct FeatureSummary {
    /// Total number of nodes that have this feature.
    pub total_nodes: u32,
    /// Number of nodes with this feature that are currently idle.
    pub idle_nodes: u32,
    /// Total number of CPUs on all nodes with this feature.
    pub total_cpus: u32,
    /// Number of CPUs on idle nodes that have this feature.
    pub idle_cpus: u32,
}

/// The final data structure that holds the summary report.
/// It's a map from a feature name (e.g., "genoa", "h100") to its aggregated stats.
pub type SummaryReportData = HashMap<String, FeatureSummary>;


// --- Aggregation Logic ---

/// Builds a feature-centric summary report of node and CPU availability.
///
/// This function iterates through all nodes and aggregates statistics based on
/// each feature a node possesses. This provides a simple overview of resource
/// availability per feature.
///
/// # Arguments
///
/// * `nodes` - A reference to the fully loaded `SlurmNodes` collection.
///
/// # Returns
///
/// A `SummaryReportData` HashMap containing the aggregated information for display.
pub fn build_summary_report(nodes: &SlurmNodes) -> SummaryReportData {
    let mut report = SummaryReportData::new();

    // Iterate through every node in the cluster.
    for node in nodes.nodes.values() {
        // Determine if the node is considered "available" or "idle".
        // For this summary, we'll consider a simple IDLE state as available.
        // We can make this logic more complex later if needed (e.g., include MIXED).
        let is_idle = node.state == NodeState::Idle;

        // A single node can have multiple features. We want to count it
        // in the summary for each of its features.
        for feature in &node.features {
            // Get the summary for this feature, or create a new one if it's the first time we've seen it.
            let summary = report.entry(feature.clone()).or_default();

            // Update the statistics.
            summary.total_nodes += 1;
            summary.total_cpus += node.cpus as u32;

            if is_idle {
                summary.idle_nodes += 1;
                summary.idle_cpus += node.cpus as u32;
            }
        }
    }

    report
}

/// Formats and prints the feature summary report to the console.
pub fn print_summary_report(summary_data: &SummaryReportData) {
    // --- Pass 1: Pre-calculate column widths ---
    let mut max_feature_width = "FEATURE".len();
    for feature_name in summary_data.keys() {
        max_feature_width = max_feature_width.max(feature_name.len());
    }
    // Add some padding
    max_feature_width += 2;

    // --- Sort features for consistent output ---
    let mut sorted_features: Vec<&String> = summary_data.keys().collect();
    sorted_features.sort();

    // --- Print Headers ---
    println!(
        "{:<width$} {:>12} {:>12}",
        "FEATURE".bold(),
        "IDLE NODES".bold(),
        "IDLE CPUS".bold(),
        width = max_feature_width
    );
    println!("{}", "-".repeat(max_feature_width + 28));

    // --- Print Report Data ---
    for feature_name in sorted_features {
        if let Some(summary) = summary_data.get(feature_name) {
            let node_stats = format!("{}/{}", summary.idle_nodes, summary.total_nodes);
            let cpu_stats = format!("{}/{}", summary.idle_cpus, summary.total_cpus);

            println!(
                "{:<width$} {:>12} {:>12}",
                feature_name,
                node_stats,
                cpu_stats,
                width = max_feature_width
            );
        }
    }
}
