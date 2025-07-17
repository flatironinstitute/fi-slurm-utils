use crate::nodes::{NodeState, Node};
use crate::jobs::SlurmJobs;
use fi_slurm::utils::count_blocks;
use std::collections::HashMap;
use colored::*;

/// Represents the aggregated statistics for a single line in the final report
///
/// This struct is used for both the main summary lines (e.g., "IDLE 197...")
/// and the indented subgroup lines (e.g., "  genoa  8...")
///
/// `#[derive(Default)]` allows us to easily create a new, zeroed-out instance
#[derive(Default, Debug, Clone)]
pub struct ReportLine {
    pub node_count: u32,
    pub total_cpus: u32,
    pub alloc_cpus: u32,
    pub idle_cpus: u32,
    pub total_gpus: u64,
    pub alloc_gpus: u64,
    pub idle_gpus: u64,
    pub node_names: Vec<String>,
}


/// Represents a top-level group in the report, categorized by a `NodeState`
///
/// For example, this would hold all the data for the "IDLE" or "MIXED" sections
#[derive(Default, Debug, Clone)]
pub struct ReportGroup {
    /// The aggregated statistics for the main summary line of this group
    pub summary: ReportLine,
    /// A map holding the statistics for each subgroup, keyed by feature or GRES name
    /// For example: `{"genoa": ReportLine, "h100": ReportLine}`
    pub subgroups: HashMap<String, ReportLine>,
}

/// The final data structure that holds the entire report, organized by `NodeState`
///
/// This will be a map from a `NodeState` enum to its corresponding `ReportGroup`
/// Using a HashMap makes it easy to add stats to the correct group as we
/// iterate through all the nodes
pub type ReportData = HashMap<NodeState, ReportGroup>;

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

/// Builds the final report by aggregating data from all nodes
///
/// This function iterates through every node, categorizes it by its state,
/// and calculates summary statistics for each state and its subgroups
///
/// # Arguments
///
/// * `nodes` - A reference to the fully loaded `SlurmNodes` collection
/// * `jobs` - A reference to the fully loaded `SlurmJobs` collection
/// * `node_to_job_map` - A map from node names to the jobs running on them
///
/// # Returns
///
/// A `ReportData` HashMap containing all the aggregated information for display
pub fn build_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<usize, Vec<u32>>,
    show_node_names: bool,
    allocated: bool,
) -> ReportData {
    let mut report_data = ReportData::new();

    // This creates a new Vec<u32> where each element corresponds to a node in the input `n` slice
    let alloc_cpus_per_node: Vec<u32> = nodes
        .iter()
        .map(|&node| {
            // For each node, look up its job IDs, returning an option
            node_to_job_map
                .get(&node.id)
                // If the Option is Some(job_ids), we map over it to perform the calculation
                .map(|job_ids| {
                    job_ids
                        .iter()
                        // For each job_id, look up the job details. filter_map unwraps the Some results
                        .filter_map(|job_id| jobs.jobs.get(job_id))
                        // For each valid job, calculate the CPUs allocated to this specific node
                        .map(|job| {
                            if job.num_nodes > 0 {
                                job.num_cpus / job.num_nodes
                            } else {
                                job.num_cpus // Should not happen, but handles malformed job data
                            }
                        })
                        .sum() // Sum the CPUs for all jobs on this node
                })
                // If the initial .get() returned None (no jobs on the node), default to 0
                .unwrap_or(0)
        })
        .collect(); // Collect all the results into our vector

    for (node, &alloc_cpus_for_node) in nodes.iter().zip(alloc_cpus_per_node.iter()) {
        // Slurm does not mark nodes as mixed by default, so we have to do it
        let derived_state = if alloc_cpus_for_node > 0 && alloc_cpus_for_node < node.cpus as u32 {
            match &node.state {
                NodeState::Compound { base, flags } => NodeState::Compound { base: Box::new(NodeState::Mixed), flags: flags.to_vec()},
                _ => NodeState::Mixed,
            }
        } else {
            // Otherwise, we trust the state reported by Slurm
            node.state.clone()
        };

        // Get the report group for the node's derived state
        let group = report_data.entry(derived_state.clone()).or_default();

        // --- Update the Main Summary Line for the Group ---
        group.summary.node_count += 1;
        group.summary.total_cpus += node.cpus as u32;
        group.summary.alloc_cpus += alloc_cpus_for_node;
        if show_node_names { group.summary.node_names.push(node.name.clone()); }
        if let Some(gpu) = &node.gpu_info {
            group.summary.total_gpus += gpu.total_gpus;
            group.summary.alloc_gpus += gpu.allocated_gpus;
        }

        // --- Determine this node's contribution to idle resources ---
        // This logic now directly implements the specified rules.
        let (idle_cpus_for_node, idle_gpus_for_node) = if !allocated {
            let base_state = match &derived_state {
                NodeState::Compound { base, .. } => base,
                _ => &derived_state,
            };

            match base_state {
                // For Idle and Mixed nodes, idle resources are what's not allocated.
                NodeState::Idle | NodeState::Mixed => {
                    let cpus = node.cpus as u32 - alloc_cpus_for_node;
                    let gpus = if let Some(gpu) = &node.gpu_info {
                        gpu.total_gpus - gpu.allocated_gpus
                    } else {
                        0
                    };
                    (cpus, gpus)
                }
                // For any other state (Allocated, Down, etc.), no resources are considered idle.
                _ => (0, 0),
            }
        } else {
            // If we're in allocated mode, idle counts are not needed.
            (0, 0)
        };
        
        // Add the calculated idle resources to the summary totals
        group.summary.idle_cpus += idle_cpus_for_node;
        group.summary.idle_gpus += idle_gpus_for_node;


        // --- Update Subgroups (GPU or Feature) ---
        if let Some(gpu) = &node.gpu_info {
            let subgroup_line = group.subgroups.entry(gpu.name.clone()).or_default();
            
            subgroup_line.node_count += 1;
            subgroup_line.total_cpus += node.cpus as u32;
            subgroup_line.alloc_cpus += alloc_cpus_for_node;
            subgroup_line.total_gpus += gpu.total_gpus;
            subgroup_line.alloc_gpus += gpu.allocated_gpus;
            if show_node_names { subgroup_line.node_names.push(node.name.clone()); }
            
            // Add this node's idle contribution to the subgroup
            subgroup_line.idle_cpus += idle_cpus_for_node;
            subgroup_line.idle_gpus += idle_gpus_for_node;

        } else if let Some(feature) = node.features.first() {
            let subgroup_line = group.subgroups.entry(feature.clone()).or_default();
            
            subgroup_line.node_count += 1;
            subgroup_line.total_cpus += node.cpus as u32;
            subgroup_line.alloc_cpus += alloc_cpus_for_node;
            if show_node_names { subgroup_line.node_names.push(node.name.clone()); }

            // Add this node's idle contribution to the subgroup
            subgroup_line.idle_cpus += idle_cpus_for_node;
        }
    };
    report_data
}

pub struct ReportWidths {
    state_width: usize,
    count_width: usize,
    alloc_or_idle_cpu_width: usize,
    total_cpu_width: usize,
    alloc_or_idle_gpu_width: usize,
    total_gpu_width: usize,
}

pub fn get_report_widths(report_data: &ReportData, allocated: bool) -> (ReportWidths, ReportLine) {
    // First, calculate the grand totals to ensure columns are wide enough.
    let mut total_line = ReportLine::default();
    for group in report_data.values() {
        total_line.node_count += group.summary.node_count;
        total_line.total_cpus += group.summary.total_cpus;
        total_line.alloc_cpus += group.summary.alloc_cpus;
        total_line.idle_cpus += group.summary.idle_cpus;
        total_line.total_gpus += group.summary.total_gpus;
        total_line.alloc_gpus += group.summary.alloc_gpus;
        total_line.idle_gpus += group.summary.idle_gpus;
    }

    // Use the totals to set the initial minimum widths.
    let initial_widths = ReportWidths {
        state_width: "STATE".len(),
        count_width: total_line.node_count.to_string().len(),
        alloc_or_idle_cpu_width: if allocated {
            total_line.alloc_cpus.to_string().len()
        } else {
            total_line.idle_cpus.to_string().len()
        },
        total_cpu_width: total_line.total_cpus.to_string().len(),
        alloc_or_idle_gpu_width: if allocated {
            total_line.alloc_gpus.to_string().len()
        } else {
            total_line.idle_gpus.to_string().len()
        },
        total_gpu_width: total_line.total_gpus.to_string().len(),
    };

    // Now, fold over the data to see if any individual line needs more space.
    let final_widths = report_data.iter().fold(
        initial_widths,
        |mut acc_widths, (state, group)| {
            acc_widths.state_width = acc_widths.state_width.max(state.to_string().len());

            let mut check_line = |line: &ReportLine| {
                acc_widths.count_width = acc_widths.count_width.max(line.node_count.to_string().len());
                
                if allocated {
                    acc_widths.alloc_or_idle_cpu_width = acc_widths.alloc_or_idle_cpu_width.max(line.alloc_cpus.to_string().len());
                    acc_widths.alloc_or_idle_gpu_width = acc_widths.alloc_or_idle_gpu_width.max(line.alloc_gpus.to_string().len());
                } else {
                    acc_widths.alloc_or_idle_cpu_width = acc_widths.alloc_or_idle_cpu_width.max(line.idle_cpus.to_string().len());
                    acc_widths.alloc_or_idle_gpu_width = acc_widths.alloc_or_idle_gpu_width.max(line.idle_gpus.to_string().len());
                }
                
                acc_widths.total_cpu_width = acc_widths.total_cpu_width.max(line.total_cpus.to_string().len());
                acc_widths.total_gpu_width = acc_widths.total_gpu_width.max(line.total_gpus.to_string().len());
            };

            check_line(&group.summary);

            for (subgroup_name, subgroup_line) in &group.subgroups {
                acc_widths.state_width = acc_widths.state_width.max(subgroup_name.len() + 2); // +2 for indentation
                check_line(subgroup_line);
            }
            acc_widths
        },
    );

    (final_widths, total_line)
}

// pub fn get_report_widths(report_data: &ReportData, allocated: bool) -> ReportWidths {
//     let initial_widths = ReportWidths {
//         state_width: "STATE".len(),
//         count_width: "COUNT".len(),
//         alloc_or_idle_cpu_width: 0, // These start at 0
//         total_cpu_width: 0,
//         alloc_or_idle_gpu_width: 0,
//         total_gpu_width: 0,
//     };
//
//     let final_widths = report_data.iter().fold(
//         initial_widths,
//         |mut acc_widths, (state, group)| {
//             // Check the width of the main state string
//             acc_widths.state_width = acc_widths.state_width.max(state.to_string().len());
//
//             let mut check_line = |line: &ReportLine| {
//                 acc_widths.count_width = acc_widths.count_width.max(line.node_count.to_string().len());
//
//                 if allocated {
//                     acc_widths.alloc_or_idle_cpu_width = acc_widths.alloc_or_idle_cpu_width.max(line.alloc_cpus.to_string().len());
//                     acc_widths.alloc_or_idle_gpu_width = acc_widths.alloc_or_idle_gpu_width.max(line.alloc_gpus.to_string().len());
//                 } else {
//                     acc_widths.alloc_or_idle_cpu_width = acc_widths.alloc_or_idle_cpu_width.max(line.idle_cpus.to_string().len());
//                     acc_widths.alloc_or_idle_gpu_width = acc_widths.alloc_or_idle_gpu_width.max(line.idle_gpus.to_string().len());
//                 }
//
//                 acc_widths.total_cpu_width = acc_widths.total_cpu_width.max(line.total_cpus.to_string().len());
//                 acc_widths.total_gpu_width = acc_widths.total_gpu_width.max(line.total_gpus.to_string().len());
//             };
//
//             // Check the summary line for the group
//             check_line(&group.summary);
//
//             // Also check every subgroup line
//             // if this isn't as snappy as we would like, remove this, though I doubt it will make
//             // much of a difference
//             for (subgroup_name, subgroup_line) in &group.subgroups {
//                 // Check the width of the subgroup name (with indentation)
//                 acc_widths.state_width = acc_widths.state_width.max(subgroup_name.len() + 2);
//                 check_line(subgroup_line);
//             }
//
//             // Return the updated accumulator for the next iteration
//             acc_widths
//         },
//     );
//
//     final_widths
// }

/// Component for the left-most column (State or Feature name).
struct StateComponent {
    colored_text: ColoredString,
    padding: String,
}

impl StateComponent {
    fn new(name: String, width: usize, no_color: bool, state: Option<&NodeState>) -> Self {
        let padding = " ".repeat(width.saturating_sub(name.len()));
        let colored_text = if no_color {
            name.normal()
        } else if let Some(s) = state {
             match s {
                NodeState::Compound { base, flags } => {
                    let base_str = base.to_string();
                    let flags_str = format!("+{}", flags.join("+").to_uppercase());
                    let colored_base = match **base {
                        NodeState::Idle => base_str.green(),
                        NodeState::Mixed => base_str.blue(),
                        NodeState::Allocated => base_str.yellow(),
                        NodeState::Down => base_str.red(),
                        NodeState::Error => base_str.magenta(),
                        _ => base_str.cyan(),
                    };
                    format!("{}{}", colored_base, flags_str).normal()
                }
                _ => {
                    match s {
                        NodeState::Idle => name.green(),
                        NodeState::Mixed => name.blue(),
                        NodeState::Allocated => name.yellow(),
                        NodeState::Down => name.red(),
                        NodeState::Error => name.magenta(),
                        _ => name.dimmed(),
                    }
                }
            }
        } else {
            name.normal() // Make TOTAL bold
        };
        Self { colored_text, padding }
    }
}

/// Component for the node count column.
struct CountComponent {
    text: String,
}
impl CountComponent {
    fn new(count: u32, width: usize) -> Self {
        Self { text: format!("{:>width$}", count, width = width) }
    }
}

/// Component for the CPU statistics column.
struct CPUComponent {
    text: String,
}
impl CPUComponent {
    fn new(line: &ReportLine, widths: &ReportWidths, allocated: bool) -> Self {
        let val = if allocated { line.alloc_cpus } else { line.idle_cpus };
        let text = format!(
            "{:>alloc_w$}/{:>total_w$}",
            val,
            line.total_cpus,
            alloc_w = widths.alloc_or_idle_cpu_width,
            total_w = widths.total_cpu_width
        );
        Self { text }
    }
}

/// Component for the GPU statistics column.
struct GPUComponent {
    text: String,
}
impl GPUComponent {
     fn new(line: &ReportLine, widths: &ReportWidths, allocated: bool) -> Self {
        if line.total_gpus == 0 {
            let total_width = widths.alloc_or_idle_gpu_width + widths.total_gpu_width + 3;
            return Self { text: format!("{:^width$}", "-", width = total_width) };
        }
        let val = if allocated { line.alloc_gpus } else { line.idle_gpus };
        let text = format!(
            "{:>alloc_w$}/{:>total_w$}",
            val,
            line.total_gpus,
            alloc_w = widths.alloc_or_idle_gpu_width,
            total_w = widths.total_gpu_width
        );
        Self { text }
    }
}

/// Formats and prints the aggregated report data to the console
pub fn print_report(report_data: &ReportData, no_color: bool, _show_node_names: bool, allocated: bool) {
    let padding: usize = 3;
    let padding_str = " ".repeat(padding);

    let (report_widths, total_line) = get_report_widths(report_data, allocated);

    let state_order: HashMap<NodeState, usize> = [
        (NodeState::Idle, 0),
        (NodeState::Mixed, 1),
        (NodeState::Allocated, 2),
        (NodeState::Error, 3),
        (NodeState::Down, 4),
    ]
    .iter()
    .cloned()
    .collect();

    let mut sorted_states: Vec<&NodeState> = report_data.keys().collect();
    sorted_states.sort_by_key(|a| {
        if let NodeState::Compound { base, .. } = a {
            state_order.get(base).unwrap_or(&99)
        } else {
            state_order.get(a).unwrap_or(&99)
        }
    });

    // --- Print Headers ---
    let cpu_header = if allocated { "CPU (A/T)" } else { "CPU (I/T)" };
    let gpu_header = if allocated { "GPU (A/T)" } else { "GPU (I/T)" };

    let cpu_header_width = report_widths.alloc_or_idle_cpu_width + report_widths.total_cpu_width + 1;
    let gpu_header_width = report_widths.alloc_or_idle_gpu_width + report_widths.total_gpu_width + 1;
    
    print!("{:<width$}", "STATE".bold(), width = report_widths.state_width + padding);
    print!("{:>width$}", "COUNT".bold(), width = report_widths.count_width + padding);
    print!("{:>width$}", cpu_header.bold(), width = cpu_header_width + padding);
    println!("{:>width$}", gpu_header.bold(), width = gpu_header_width + padding);

    let total_width = (report_widths.state_width + padding) + (report_widths.count_width + padding) + (cpu_header_width + padding) + (gpu_header_width + padding);
    println!("{}", "-".repeat(total_width));

    // --- Print Report Body ---
    for state in sorted_states {
        if let Some(group) = report_data.get(state) {
            let state_comp = StateComponent::new(state.to_string(), report_widths.state_width, no_color, Some(state));
            let count_comp = CountComponent::new(group.summary.node_count, report_widths.count_width);
            let cpu_comp = CPUComponent::new(&group.summary, &report_widths, allocated);
            let gpu_comp = GPUComponent::new(&group.summary, &report_widths, allocated);

            // FIX: Print each component separately to ensure alignment.
            print!("{}{}", state_comp.colored_text, state_comp.padding);
            print!("{}", padding_str);
            print!("{}", count_comp.text);
            print!("{}", padding_str);
            print!("{}", cpu_comp.text);
            print!("{}", padding_str);
            println!("{}", gpu_comp.text);

            let mut sorted_subgroups: Vec<&String> = group.subgroups.keys().collect();
            sorted_subgroups.sort();

            for subgroup_name in sorted_subgroups {
                if let Some(line) = group.subgroups.get(subgroup_name) {
                    let state_comp = StateComponent::new(format!("  {}", subgroup_name), report_widths.state_width, no_color, None);
                    let count_comp = CountComponent::new(line.node_count, report_widths.count_width);
                    let cpu_comp = CPUComponent::new(line, &report_widths, allocated);
                    let gpu_comp = GPUComponent::new(line, &report_widths, allocated);
                    
                    print!("{}{}", state_comp.colored_text, state_comp.padding);
                    print!("{}", padding_str);
                    print!("{}", count_comp.text);
                    print!("{}", padding_str);
                    print!("{}", cpu_comp.text);
                    print!("{}", padding_str);
                    println!("{}", gpu_comp.text);
                }
            }
        }
    }
    println!("{}", "-".repeat(total_width));
    let state_comp = StateComponent::new("TOTAL".to_string(), report_widths.state_width, no_color, None);
    let count_comp = CountComponent::new(total_line.node_count, report_widths.count_width);
    let cpu_comp = CPUComponent::new(&total_line, &report_widths, allocated);
    let gpu_comp = GPUComponent::new(&total_line, &report_widths, allocated);

    print!("{}{}", state_comp.colored_text, state_comp.padding);
    print!("{}", padding_str);
    print!("{}", count_comp.text);
    print!("{}", padding_str);
    print!("{}", cpu_comp.text);
    print!("{}", padding_str);
    println!("{}", gpu_comp.text);
}

/// Formats a statistics column (e.g., "5 / 100") with digit alignment
fn format_stat_column(
    alloc: u64,
    total: u64,
    max_alloc_width: usize,
    max_total_width: usize,
) -> String {
    format!(
        "{:>alloc_w$}/{:>total_w$}",
        alloc,
        total,
        alloc_w = max_alloc_width,
        total_w = max_total_width -1
    )
}

fn get_node_utilization(report_data: &HashMap<NodeState, ReportGroup>) -> f64 {
    let utilized_nodes = report_data.iter().fold(0, |account, (state, group)| {
        let is_utilized = match state {
            NodeState::Allocated | NodeState::Mixed => true,
            NodeState::Compound{ base, ..} => matches!(**base, NodeState::Allocated | NodeState::Mixed),
            _ => false,
        };
        if is_utilized { account + group.summary.node_count } else { account }
    });
    utilized_nodes as f64
}

#[derive(Clone)]
enum BarColor {
    Red,
    Green,
    Cyan
}

impl BarColor {
    pub fn apply_color(&self, text: &str) -> ColoredString {
        match self {
            BarColor::Cyan => text.cyan(),
            BarColor::Red => text.red(),
            BarColor::Green => text.green()
        }
    }
}

fn print_utilization(utilization_percent: f64, bar_width: usize, bar_color: BarColor, name: &str, no_color: bool, allocated: bool) {
    // Call count_blocks to get the components of the bar
    let (full, empty, partial_opt) = count_blocks(bar_width, utilization_percent / 100.0);

    // Create the string for the full blocks
    let full_bar = "█".repeat(full);

    // Get the partial block character, or an empty string if there isn't one
    let partial_bar = partial_opt.unwrap_or_default();

    // Create the string for the empty space. Using a simple space is often cleaner
    let empty_bar = " ".repeat(empty);

    // Apply color to the filled parts of the bar
    let colored_full = if no_color { full_bar.white() } else { bar_color.apply_color(&full_bar) };
    let colored_partial = if no_color { partial_bar.white() } else { bar_color.apply_color(&partial_bar) };

    // Print the assembled bar
    if allocated {
        println!(
            "Overall {} Utilization: \n [{}{}{}] {:.1}%",
            name,
            colored_full,
            colored_partial,
            empty_bar, // The empty part is not colored.
            utilization_percent
        );
    } else {
        println!(
            "Overall {} Availability: \n [{}{}{}] {:.1}%",
            name,
            colored_full,
            colored_partial,
            empty_bar, // The empty part is not colored.
            utilization_percent
        );
    }
}

fn generate_report_body(
    report_data: &HashMap<NodeState, ReportGroup>,
    sorted_states: Vec<&NodeState>,
    max_state_width: usize,
    widths: (usize, usize, usize, usize, usize, usize),
    no_color: bool,
    show_node_names: bool,
    allocated: bool,
) -> ReportLine {
    
    let (max_alloc_cpu_width, max_total_cpu_width, max_alloc_gpu_width, max_total_gpu_width, max_idle_cpu_width, max_idle_gpu_width) = widths;

    let mut total_line = ReportLine::default();
    for state in sorted_states {
        if let Some(group) = report_data.get(state) {
            total_line.node_count += group.summary.node_count;
            total_line.total_cpus += group.summary.total_cpus;
            total_line.alloc_cpus += group.summary.alloc_cpus;
            total_line.total_gpus += group.summary.total_gpus;
            total_line.alloc_gpus += group.summary.alloc_gpus;

            let uncolored_str = state.to_string();
            let padding = " ".repeat(max_state_width.saturating_sub(uncolored_str.len()));
            
            let colored_str = {
                let colorize = |s: &NodeState, text: String| -> ColoredString {
                    if no_color { return text.normal(); }
                    match s {
                        NodeState::Idle => text.green(),
                        NodeState::Mixed => text.cyan(),
                        NodeState::Allocated => text.yellow(),
                        NodeState::Down => text.red(),
                        NodeState::Error => text.magenta(),
                        _ => text.white(),
                    }
                };
                if let NodeState::Compound { base, flags } = state {
                    let base_str = base.to_string();
                    let flags_str = format!("+{}", flags.join("+").to_uppercase());
                    format!("{}{}", colorize(base, base_str), flags_str)
                } else {
                    colorize(state, uncolored_str.clone()).to_string()
                }
            };

            // current CPU/GPU values based on mode
            let cur_cpu = if allocated { group.summary.alloc_cpus } else { group.summary.total_cpus.saturating_sub(group.summary.alloc_cpus) };
            let cpu_w = if allocated { max_alloc_cpu_width } else { max_idle_cpu_width };
            let cpu_str = format_stat_column(cur_cpu as u64, group.summary.total_cpus as u64, cpu_w, max_total_cpu_width);
            let cur_gpu = if allocated { group.summary.alloc_gpus } else { group.summary.total_gpus.saturating_sub(group.summary.alloc_gpus) };
            let gpu_str = if group.summary.total_gpus > 0 {
                format_stat_column(cur_gpu, group.summary.total_gpus, if allocated { max_alloc_gpu_width } else { max_idle_gpu_width }, max_total_gpu_width)
            } else {
                let total_width = if allocated { max_alloc_gpu_width } else { max_idle_gpu_width } + max_total_gpu_width;
                format!("{:^width$}", " -  ", width = total_width)
            };
            
            // Use a two-step print to ensure alignment with colored text.
            print!("{}{}", colored_str, padding);

            println!("{:>5}  {}  {}  {}", 
                group.summary.node_count, 
                cpu_str, 
                gpu_str, 
                if show_node_names {
                    fi_slurm::parser::compress_hostlist(&group.summary.node_names)
                } else {
                    "".to_string()
                },
            );

            let mut sorted_subgroups: Vec<&String> = group.subgroups.keys().collect();
            sorted_subgroups.sort();

            for subgroup_name in sorted_subgroups {
                if let Some(subgroup_line) = group.subgroups.get(subgroup_name) {
                    let sub_cur_cpu = if allocated { subgroup_line.alloc_cpus } else { subgroup_line.total_cpus.saturating_sub(subgroup_line.alloc_cpus) };
                    let sub_cpu_w = if allocated { max_alloc_cpu_width } else { max_idle_cpu_width };
                    let sub_cpu_str = format_stat_column(sub_cur_cpu as u64, subgroup_line.total_cpus as u64, sub_cpu_w, max_total_cpu_width);
                    let sub_cur_gpu = if allocated { subgroup_line.alloc_gpus } else { subgroup_line.total_gpus.saturating_sub(subgroup_line.alloc_gpus) };
                    let sub_gpu_str = if subgroup_line.total_gpus > 0 {
                        format_stat_column(sub_cur_gpu, subgroup_line.total_gpus, if allocated { max_alloc_gpu_width } else { max_idle_gpu_width }, max_total_gpu_width)
                    } else {
                        let total_width = if allocated { max_alloc_gpu_width } else { max_idle_gpu_width } + max_total_gpu_width;
                        format!("{:^width$}", " -  ", width = total_width)
                    };
                    
                    let indented_name = format!("  {}", subgroup_name);
                    let sub_padding = " ".repeat(max_state_width.saturating_sub(indented_name.len()));

                    print!("{}{}", indented_name, sub_padding);
    
                    println!("{:>5}  {}  {}  {}", 
                        subgroup_line.node_count, 
                        sub_cpu_str, 
                        sub_gpu_str, 
                        if show_node_names {
                            fi_slurm::parser::compress_hostlist(&subgroup_line.node_names)
                        } else {
                            "".to_string()
                        }
                    );
                }
            }
        }
    }
    total_line
}
