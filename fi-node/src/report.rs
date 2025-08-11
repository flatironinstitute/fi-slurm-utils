use fi_slurm::nodes::{NodeState, Node};
use fi_slurm::jobs::SlurmJobs;
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
    verbose: bool,
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
                NodeState::Compound { flags, .. } => NodeState::Compound { base: Box::new(NodeState::Mixed), flags: flags.to_vec()},
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
            let subgroup_key = if !verbose && gpu.name.starts_with("gpu:") {
                "gpu".to_string()
            } else {
                gpu.name.clone()
            };
            
            let subgroup_line = group.subgroups.entry(subgroup_key).or_default();
            
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
            return Self { text: format!("{:^width$}", "-", width = total_width-2) };
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
pub fn print_report(report_data: &ReportData, no_color: bool, show_node_names: bool, allocated: bool) {
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

    let flag_order: HashMap<&str, usize> = [
        ("EXTERNAL", 0), ("RES", 1), ("UNDRAIN", 2), ("CLOUD", 3),
        ("RESUME", 4), ("DRAIN", 5), ("COMPLETING", 6), ("NO_RESPOND", 7),
        ("POWERED_DOWN", 8), ("FAIL", 9), ("POWERING_UP", 10), ("MAINT", 11),
        ("REBOOT_REQUESTED", 12), ("REBOOT_CANCEL", 13), ("POWERING_DOWN", 14),
        ("DYNAMIC_FUTURE", 15), ("REBOOT_ISSUED", 16), ("PLANNED", 17),
        ("INVALID_REG", 18), ("POWER_DOWN", 19), ("POWER_UP", 20),
        ("POWER_DRAIN", 21), ("DYNAMIC_NORM", 22), ("BLOCKED", 23)
    ].iter().cloned().collect();

    let mut sorted_states: Vec<&NodeState> = report_data.keys().collect();
    sorted_states.sort_by(|a, b| {
        let to_key = |state: &&NodeState| {
            let (base_state, flags) = match state {
                NodeState::Compound { base, flags } => (base.as_ref(), flags),
                _ => (*state, &Vec::new()), // Treat simple state as having no flags
            };
            let base_priority = *state_order.get(base_state).unwrap_or(&99);

            let mut flag_priorities: Vec<usize> = flags
                .iter()
                .map(|f| *flag_order.get(f.to_uppercase().as_str()).unwrap_or(&99))
                .collect();
            flag_priorities.sort_unstable(); // Sort by priority numbers for canonical comparison

            (base_priority, flag_priorities)
        };

        to_key(a).cmp(&to_key(b))
    });

    // --- Print Headers ---
    
    let cpu_header = if allocated { "CPU (A/T)" } else { "CPU (I/T)" };
    let gpu_header = if allocated { "GPU (A/T)" } else { "GPU (I/T)" };
    
    // Calculate the exact width of the data part of each column
    let count_data_width = report_widths.count_width;
    let cpu_data_width = report_widths.alloc_or_idle_cpu_width + report_widths.total_cpu_width + 1;
    let gpu_data_width = report_widths.alloc_or_idle_gpu_width + report_widths.total_gpu_width + 1;

    // Format each header to be aligned within its data column's width
    let state_header_formatted = format!("{:<width$}", "STATE".bold(), width = report_widths.state_width);
    let count_header_formatted = format!("{:<width$}", "COUNT".bold(), width = count_data_width);
    let cpu_header_formatted = format!("{:<width$}", cpu_header.bold(), width = cpu_data_width);
    let gpu_header_formatted = format!("{:<width$}", gpu_header.bold(), width = gpu_data_width);

    // Print each formatted header followed by the padding string, mirroring the data row printing
    print!("{}{}", state_header_formatted, padding_str);
    print!("{}{}", count_header_formatted, padding_str);
    print!("{}{}", cpu_header_formatted, padding_str);
    println!("{}", gpu_header_formatted); // No padding at the end of the line

    let total_width = report_widths.state_width + padding_str.len() + count_data_width + padding_str.len() + cpu_data_width + padding_str.len() + gpu_data_width;
    println!("{}", "-".repeat(total_width + padding));

    // --- Print Report Body ---
    for state in sorted_states {
        if let Some(group) = report_data.get(state) {
            let state_comp = StateComponent::new(state.to_string(), report_widths.state_width, no_color, Some(state));
            let count_comp = CountComponent::new(group.summary.node_count, report_widths.count_width);
            let cpu_comp = CPUComponent::new(&group.summary, &report_widths, allocated);
            let gpu_comp = GPUComponent::new(&group.summary, &report_widths, allocated);
            let node_names = &group.summary.node_names.clone();

        println!(
            "{}{}{}{}{}{}{}{} | {}",
            state_comp.colored_text,
            state_comp.padding,
            padding_str,
            count_comp.text,
            padding_str,
            cpu_comp.text,
            padding_str,
            gpu_comp.text,
            if show_node_names {fi_slurm::parser::compress_hostlist(node_names)} else {"".to_string()}
        );

            // // FIX: Print each component separately to ensure alignment.
            // print!("{}{}", state_comp.colored_text, state_comp.padding);
            // print!("{}", padding_str);
            // print!("{}", count_comp.text);
            // print!("{}", padding_str);
            // print!("{}", cpu_comp.text);
            // print!("{}", padding_str);
            // println!("{}", gpu_comp.text);
            // println!("{}", if show_node_names {fi_slurm::parser::compress_hostlist(node_names)} else {"".to_string()});

            let mut sorted_subgroups: Vec<&String> = group.subgroups.keys().collect();
            sorted_subgroups.sort();

            for subgroup_name in sorted_subgroups {
                if let Some(line) = group.subgroups.get(subgroup_name) {
                    let state_comp = StateComponent::new(format!("  {}", subgroup_name), report_widths.state_width, no_color, None);
                    let count_comp = CountComponent::new(line.node_count, report_widths.count_width);
                    let cpu_comp = CPUComponent::new(line, &report_widths, allocated);
                    let gpu_comp = GPUComponent::new(line, &report_widths, allocated);
                    let node_names = &line.node_names.clone();
                    
                    println!(
                        "{}{}{}{}{}{}{}{} | {}",
                        state_comp.colored_text,
                        state_comp.padding,
                        padding_str,
                        count_comp.text,
                        padding_str,
                        cpu_comp.text,
                        padding_str,
                        gpu_comp.text,
                        if show_node_names {fi_slurm::parser::compress_hostlist(node_names)} else {"".to_string()}
                    );
                    // print!("{}{}", state_comp.colored_text, state_comp.padding);
                    // print!("{}", padding_str);
                    // print!("{}", count_comp.text);
                    // print!("{}", padding_str);
                    // print!("{}", cpu_comp.text);
                    // print!("{}", padding_str);
                    // println!("{}", gpu_comp.text);
                    // println!("{}", if show_node_names {fi_slurm::parser::compress_hostlist(node_names)} else {"".to_string()});
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


    // Utilization bars: show allocated or idle based on flag

    print_utilization_bars(report_data, &total_line, allocated, no_color);
}

fn print_utilization_bars(report_data: &ReportData, total_line: &ReportLine, allocated: bool, no_color: bool) {
    println!(); // Add a blank line for spacing
    if allocated {
        // --- Utilization ---
        if total_line.node_count > 0 {
            let utilized_nodes = report_data.iter().fold(0, |acc, (state, group)| {
                let base_state = match state {
                    NodeState::Compound { base, .. } => base,
                    _ => state,
                };
                if matches!(*base_state, NodeState::Allocated | NodeState::Mixed) {
                    acc + group.summary.node_count
                } else {
                    acc
                }
            });
            let percent = (utilized_nodes as f64 / total_line.node_count as f64) * 100.0;
            print_utilization(percent, 50, BarColor::Green, "Node", no_color, allocated);
        }
        if total_line.total_cpus > 0 {
            let percent = (total_line.alloc_cpus as f64 / total_line.total_cpus as f64) * 100.0;
            print_utilization(percent, 50, BarColor::Cyan, "CPU", no_color, allocated);
        }
        if total_line.total_gpus > 0 {
            let percent = (total_line.alloc_gpus as f64 / total_line.total_gpus as f64) * 100.0;
            print_utilization(percent, 50, BarColor::Red, "GPU", no_color, allocated);
        }
    } else {
        // --- Availability ---
        if total_line.node_count > 0 {
            let available_nodes = get_available_nodes(report_data);
            let percent = (available_nodes as f64 / total_line.node_count as f64) * 100.0;
            print_utilization(percent, 50, BarColor::Green, "Node", no_color, allocated);
        }
        if total_line.total_cpus > 0 {
            let available_cpus = get_available_cpus(report_data);
            let percent = (available_cpus as f64 / total_line.total_cpus as f64) * 100.0;
            print_utilization(percent, 50, BarColor::Cyan, "CPU", no_color, allocated);
        }
        if total_line.total_gpus > 0 {
            let available_gpus = get_available_gpus(report_data);
            let percent = (available_gpus as f64 / total_line.total_gpus as f64) * 100.0;
            print_utilization(percent, 50, BarColor::Red, "GPU", no_color, allocated);
        }
    }
}

fn is_node_available(state: &NodeState) -> bool {
    match state {
        NodeState::Idle => true,
        NodeState::Compound { base, flags } => {
            if **base == NodeState::Idle {
                // Node is idle, but check for disqualifying flags
                !flags.iter().any(|flag| {
                    let flag_str = flag.as_str();
                    flag_str == "MAINT" || flag_str == "DOWN" || flag_str == "DRAIN" || flag_str == "INVALID_REG"
                })
            } else {
                false
            }
        }
        _ => false,
    }
}

fn get_available_nodes(report_data: &ReportData) -> u32 {
    report_data.iter().fold(0, |acc, (state, group)| {
        if is_node_available(state) {
            acc + group.summary.node_count
        } else {
            acc
        }
    })
}

/// Gets the total number of CPUs on nodes that are available to run jobs.
fn get_available_cpus(report_data: &ReportData) -> u32 {
    report_data.iter().fold(0, |acc, (state, group)| {
        if is_node_available(state) {
            acc + group.summary.total_cpus
        } else {
            acc
        }
    })
}

/// Gets the total number of GPUs on nodes that are available to run jobs.
fn get_available_gpus(report_data: &ReportData) -> u64 {
    report_data.iter().fold(0, |acc, (state, group)| {
        if is_node_available(state) {
            acc + group.summary.total_gpus
        } else {
            acc
        }
    })
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

