use crate::nodes::{NodeState, SlurmNodes};
use crate::jobs::SlurmJobs;
use colored::{Color, Colorize};
use std::collections::HashMap;

/// Holds the aggregated statistics for a single feature across the entire cluster
#[derive(Default, Debug, Clone)]
pub struct FeatureSummary {
    /// Total number of nodes that have this feature
    pub total_nodes: u32,
    /// Number of nodes with this feature that are currently idle
    pub idle_nodes: u32,
    /// Total number of CPUs on all nodes with this feature
    pub total_cpus: u32,
    /// Number of CPUs on idle nodes that have this feature
    pub idle_cpus: u32,
}

/// The final data structure that holds the summary report
/// It's a map from a feature name (e.g., "genoa", "h100") to its aggregated stats
pub type SummaryReportData = HashMap<String, FeatureSummary>;


// --- Aggregation Logic ---

/// Builds a feature-centric summary report of node and CPU availability
///
/// This function iterates through all nodes and aggregates statistics based on
/// each feature a node possesses. This provides a simple overview of resource
/// availability per feature
///
/// # Arguments
///
/// * `nodes` - A reference to the fully loaded `SlurmNodes` collection
/// * `jobs` - A reference to the fully loaded `SlurmJobs` collection
/// * `node_to_job_map` - A map from node names to the jobs running on them
///
/// # Returns
///
/// A `SummaryReportData` HashMap containing the aggregated information for display
pub fn build_summary_report(
    nodes: &SlurmNodes,
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<String, Vec<u32>>,
) -> SummaryReportData {
    let mut report = SummaryReportData::new();

    // Iterate through every node in the cluster
    for node in nodes.nodes.values() {
        // First, calculate the true number of allocated and idle CPUs for this node
        let alloc_cpus_for_node: u32 = if let Some(job_ids) = node_to_job_map.get(&node.name) {
            job_ids
                .iter()
                .filter_map(|job_id| jobs.jobs.get(job_id))
                .map(|job| {
                    if job.num_nodes > 0 {
                        job.num_cpus / job.num_nodes
                    } else {
                        job.num_cpus
                    }
                })
                .sum()
        } else {
            0
        };
        let idle_cpus_for_node = (node.cpus as u32).saturating_sub(alloc_cpus_for_node);

        // A single node can have multiple features. We want to count it
        // in the summary for each of its features
        for feature in &node.features {
            let summary = report.entry(feature.clone()).or_default();

            // Update total stats regardless of state.
            summary.total_nodes += 1;
            summary.total_cpus += node.cpus as u32;

            // Add this node's actually idle CPUs to the summary
            // This now correctly includes idle CPUs from MIXED nodes
            summary.idle_cpus += idle_cpus_for_node;
            
            // Only increment the 'idle_nodes' count if the node is purely idle
            if node.state == NodeState::Idle {
                summary.idle_nodes += 1;
            }
        }
    }

    report
}

/// Defines the type of text to display inside a gauge
enum GaugeText {
    Proportion,
    Percentage,
}

/// Creates a string representing a gauge with text overlaid
fn create_gauge(current: u32, total: u32, width: usize, bar_color: Color, text_format: GaugeText) -> String {
    if total == 0 {
        return format!("{:^width$}", "-");
    }

    let percentage = current as f64 / total as f64;
    let filled_len = (width as f64 * percentage).round() as usize;
    
    // Format the text based on the requested format
    let text = match text_format {
        GaugeText::Proportion => format!("{}/{}", current, total),
        GaugeText::Percentage => format!("{:.1}%", percentage * 100.0),
    };

    let empty_color = (40, 40, 40); // Dark grey background

    let mut gauge_chars: Vec<String> = Vec::with_capacity(width);
    for _ in 0..filled_len {
        gauge_chars.push(" ".on_color(bar_color).to_string());
    }
    for _ in filled_len..width {
        gauge_chars.push(" ".on_truecolor(empty_color.0, empty_color.1, empty_color.2).to_string());
    }

    let text_start_pos = if text.len() >= width { 0 } else { (width - text.len()) / 2 };
    
    for (i, char) in text.chars().enumerate() {
        if let Some(pos) = text_start_pos.checked_add(i) {
            if pos < width {
                if pos < filled_len {
                    gauge_chars[pos] = char.to_string().white().on_color(bar_color).to_string();
                } else {
                    gauge_chars[pos] = char.to_string().white().on_truecolor(empty_color.0, empty_color.1, empty_color.2).to_string();
                }
            }
        }
    }

    gauge_chars.join("")
}


/// Formats and prints the feature summary report to the console
pub fn print_summary_report(summary_data: &SummaryReportData) {
    // Pass 1: Pre-calculate column widths 
    let mut max_feature_width = "FEATURE".len();
    for feature_name in summary_data.keys() {
        max_feature_width = max_feature_width.max(feature_name.len());
    }
    max_feature_width += 2;
    
    let gauge_width = 15;

    // Sort features for consistent output
    let mut sorted_features: Vec<&String> = summary_data.keys().collect();
    sorted_features.sort();

    // Print Headers
    println!(
        "{:<width$} {:^gauge_w$} {:^gauge_w$}",
        "FEATURE".bold(),
        "IDLE NODES".bold(),
        "IDLE CPUS".bold(),
        width = max_feature_width,
        gauge_w = gauge_width
    );
    let total_width = max_feature_width + gauge_width * 2 + 2;
    println!("{}", "-".repeat(total_width));

    // Calculate Grand Totals
    let mut total_nodes = 0;
    let mut idle_nodes = 0;
    let mut total_cpus = 0;
    let mut idle_cpus = 0;
    for summary in summary_data.values() {
        total_nodes += summary.total_nodes;
        idle_nodes += summary.idle_nodes;
        total_cpus += summary.total_cpus;
        idle_cpus += summary.idle_cpus;
    }

    // Print Report Data
    for feature_name in sorted_features {
        if let Some(summary) = summary_data.get(feature_name) {
            // Use GaugeText::Proportion for individual feature rows
            let node_gauge = create_gauge(summary.idle_nodes, summary.total_nodes, gauge_width, Color::Green, GaugeText::Proportion);
            let cpu_gauge = create_gauge(summary.idle_cpus, summary.total_cpus, gauge_width, Color::Cyan, GaugeText::Proportion);

            println!(
                "{:<width$} {} {}",
                feature_name,
                node_gauge,
                cpu_gauge,
                width = max_feature_width
            );
        }
    }

    // Print Total Line with Bars
    println!("{}", "-".repeat(total_width));
    // Use GaugeText::Percentage for the final TOTAL row
    let node_gauge = create_gauge(idle_nodes, total_nodes, gauge_width, Color::Green, GaugeText::Percentage);
    let cpu_gauge = create_gauge(idle_cpus, total_cpus, gauge_width, Color::Cyan, GaugeText::Percentage);
    println!(
        "{:<width$} {} {}",
        "TOTAL".bold(),
        node_gauge,
        cpu_gauge,
        width = max_feature_width
    );
}
