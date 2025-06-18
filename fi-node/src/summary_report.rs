use crate::nodes::{NodeState, SlurmNodes};
use colored::{Color, Colorize};
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
/// Creates a string representing a gauge with text overlaid.
///
/// # Arguments
/// * `current` - The current value (e.g., idle nodes).
/// * `total` - The total value (e.g., total nodes).
/// * `width` - The total character width of the gauge.
/// * `bar_color` - The color to use for the filled part of the gauge.
fn create_gauge(current: u32, total: u32, width: usize, bar_color: Color) -> String {
    if total == 0 {
        return format!("{:^width$}", "-");
    }

    let percentage = current as f64 / total as f64;
    let filled_len = (width as f64 * percentage).round() as usize;
    let text = format!("{}/{}", current, total);

    let empty_color = (40, 40, 40); // Dark grey background

    let mut gauge_chars: Vec<String> = Vec::with_capacity(width);
    // Build the colored bar part
    for _ in 0..filled_len {
        gauge_chars.push(" ".on_color(bar_color).to_string());
    }
    // Build the empty part
    for _ in filled_len..width {
        gauge_chars.push(" ".on_truecolor(empty_color.0, empty_color.1, empty_color.2).to_string());
    }

    // Overlay the text onto the gauge characters
    let text_start_pos = if text.len() >= width { 0 } else { (width - text.len()) / 2 };
    
    for (i, char) in text.chars().enumerate() {
        if let Some(pos) = text_start_pos.checked_add(i) {
            if pos < width {
                // FIX: Check if the character position is over the filled or empty
                // part of the bar and apply the correct background color.
                if pos < filled_len {
                    // This character is over the filled part.
                    gauge_chars[pos] = char.to_string().white().on_color(bar_color).to_string();
                } else {
                    // This character is over the empty part.
                    gauge_chars[pos] = char.to_string().white().on_truecolor(empty_color.0, empty_color.1, empty_color.2).to_string();
                }
            }
        }
    }

    gauge_chars.join("")
}



///// Creates a string representing a gauge with text overlaid.
/////
///// # Arguments
///// * `current` - The current value (e.g., idle nodes).
///// * `total` - The total value (e.g., total nodes).
///// * `width` - The total character width of the gauge.
///// * `bar_color` - The color to use for the filled part of the gauge.
//fn create_gauge(current: u32, total: u32, width: usize, bar_color: Color) -> String {
//    if total == 0 {
//        return format!("{:^width$}", "-");
//    }
//
//    let percentage = current as f64 / total as f64;
//    let filled_len = (width as f64 * percentage).round() as usize;
//    let text = format!("{}/{}", current, total);
//
//    let mut gauge_chars: Vec<String> = Vec::with_capacity(width);
//    // Build the colored bar part
//    for _ in 0..filled_len {
//        gauge_chars.push(" ".on_color(bar_color).to_string());
//    }
//    // Build the empty part
//    for _ in filled_len..width {
//        gauge_chars.push(" ".on_truecolor(40, 40, 40).to_string()); // Dark grey background
//    }
//
//    // Overlay the text onto the gauge characters
//    let text_start_pos = width.saturating_sub(text.len()) / 2;
//    for (i, char) in text.chars().enumerate() {
//        if let Some(pos) = text_start_pos.checked_add(i) {
//            if pos < width {
//                // To overlay, we replace the background color string with a text character
//                // that has the same background but a visible foreground.
//                gauge_chars[pos] = char.to_string().white().on_color(bar_color).to_string();
//            }
//        }
//    }
//
//    gauge_chars.join("")
//}

/// Formats and prints the feature summary report to the console.
pub fn print_summary_report(summary_data: &SummaryReportData) {
    // --- Pass 1: Pre-calculate column widths ---
    let mut max_feature_width = "FEATURE".len();
    for feature_name in summary_data.keys() {
        max_feature_width = max_feature_width.max(feature_name.len());
    }
    // Add some padding
    max_feature_width += 2;

    let gauge_width = 15; // Define a fixed width for our gauges

    // --- Sort features for consistent output ---
    let mut sorted_features: Vec<&String> = summary_data.keys().collect();
    sorted_features.sort();

    // --- Print Headers ---
    println!(
        "{:<width$} {:^gauge_w$} {:^gauge_w$}",
        "FEATURE".bold(),
        "IDLE NODES".bold(),
        "IDLE CPUS".bold(),
        width = max_feature_width,
        gauge_w = gauge_width,
    );
    let total_width = max_feature_width + gauge_width * 2 + 2; // +2 for spaces between columns
    println!("{}", "-".repeat(total_width));


    // --- Print Report Data ---
    for feature_name in sorted_features {
        if let Some(summary) = summary_data.get(feature_name) {
            let node_gauge = create_gauge(summary.idle_nodes, summary.total_nodes, gauge_width, Color::Green);
            let cpu_gauge = create_gauge(summary.idle_cpus, summary.total_cpus, gauge_width, Color::Cyan);

            println!(
                "{:<width$} {} {}",
                feature_name,
                node_gauge,
                cpu_gauge,
                width = max_feature_width
            );
        }
    }
}
