use crate::PreemptJobs;
use colored::*;
use fi_slurm::filter::FeatureQuery;
use fi_slurm::jobs::{Job, SlurmJobs};
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
            "sxm", "sxm2", "sxm4", "sxm5", "nvlink", "a100", "h100", "v100", "ib",
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

/// What a single node contributes to the stats of every tree level it belongs to
///
/// As in `ReportLine`, the `cpus` fields carry GPU counts when the report is counting GPUs.
#[derive(Default)]
struct NodeContribution {
    idle_nodes: u32,
    preempt_nodes: Option<u32>,
    total_cpus: u32,
    idle_cpus: u32,
    preempt_cpus: Option<u32>,
    alloc_cpus: u32,
}

impl ReportLine {
    /// Folds one node's contribution into this line
    fn add(&mut self, contribution: &NodeContribution) {
        self.total_nodes += 1;
        self.idle_nodes += contribution.idle_nodes;
        self.total_cpus += contribution.total_cpus;
        self.idle_cpus += contribution.idle_cpus;
        self.alloc_cpus += contribution.alloc_cpus;

        // `None` and `Some(0)` are deliberately distinct: the latter marks a line where
        // preemption is in play but yields nothing, which prints as "(-0)" rather than blank
        if let Some(nodes) = contribution.preempt_nodes {
            *self.preempt_nodes.get_or_insert(0) += nodes;
        }
        if let Some(cpus) = contribution.preempt_cpus {
            *self.preempt_cpus.get_or_insert(0) += cpus;
        }
    }
}

/// A job's share of one node, counted in GPUs rather than CPUs when `gpu` is set
///
/// Slurm reports these totals for the job as a whole, so an even split across its nodes is the
/// closest we can get for a heterogeneous allocation.
fn job_share(job: &Job, gpu: bool) -> u32 {
    let total = if gpu {
        job.allocated_gres.get("gres/gpu").copied().unwrap_or(0) as u32
    } else {
        job.num_cpus
    };

    total / job.num_nodes.max(1)
}

/// Computes what one node adds to every tree level it belongs to
///
/// Under `preempt_jobs`, resources held by jobs that are already preemptable count as available,
/// and the `preempt_*` fields record how much of that availability requires preempting something.
fn node_contribution(
    node: &Node,
    jobs: &SlurmJobs,
    jobs_on_node: &[u32],
    preempt_jobs: Option<&PreemptJobs>,
    gpu: bool,
) -> NodeContribution {
    let alloc_cpus: u32 = jobs_on_node
        .iter()
        .filter_map(|id| jobs.jobs.get(id))
        .map(|job| job_share(job, false))
        .sum();

    let (total, alloc) = match (gpu, &node.gpu_info) {
        (true, Some(gpu_info)) => (gpu_info.total_gpus as u32, gpu_info.allocated_gpus as u32),
        (true, None) => (0, 0),
        (false, _) => (node.cpus as u32, alloc_cpus),
    };

    // Slurm calls a node Allocated once every core is claimed, but the jobs we can see on it may
    // not account for all of them
    let derived_state = if alloc_cpus > 0 && alloc_cpus < node.cpus as u32 {
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

    // the preempt set, kept only if any of it is actually running here
    let preempt_jobs =
        preempt_jobs.filter(|preempt_jobs| jobs_on_node.iter().any(|id| preempt_jobs.contains(id)));

    // `preempt_node` has already rewritten the state of a node whose every job is preemptable to
    // Idle; the Mixed derivation above must not undo that, since preempting frees the whole node
    // however few of its cores those jobs hold
    let (is_available, is_mixed) = if preempt_jobs.is_some() && is_node_available(&node.state) {
        (true, false)
    } else {
        (
            is_node_available(&derived_state),
            is_node_mixed(&derived_state),
        )
    };

    let mut contribution = NodeContribution {
        total_cpus: total,
        alloc_cpus: alloc,
        ..Default::default()
    };

    // a down, drained or reserved node offers nothing, preemption included
    if !is_available && !is_mixed {
        return contribution;
    }

    let preemptable_held = match preempt_jobs {
        Some(preempt_jobs) => {
            let held = if jobs_on_node.iter().all(|id| preempt_jobs.contains(id)) {
                // the node's own allocation figure is exact, so prefer it over apportioned
                // per-job shares whenever everything on the node can be preempted
                alloc
            } else {
                jobs_on_node
                    .iter()
                    .filter(|id| preempt_jobs.contains(id))
                    .filter_map(|id| jobs.jobs.get(id))
                    .map(|job| job_share(job, gpu))
                    .sum::<u32>()
                    .min(alloc)
            };

            // a whole node is only obtainable if preempting clears everything on it
            contribution.preempt_nodes = Some(u32::from(is_available));
            contribution.preempt_cpus = Some(held);

            held
        }
        None => 0,
    };

    if is_available {
        contribution.idle_nodes = 1;
    }
    contribution.idle_cpus = total.saturating_sub(alloc) + preemptable_held;

    contribution
}

/// Builds a hierarchical tree report from a flat list of Slurm nodes
#[allow(clippy::too_many_arguments)]
pub fn build_tree_report(
    nodes: &[&Node],
    jobs: &SlurmJobs,
    node_to_job_map: &HashMap<usize, Vec<u32>>,
    selection: &FeatureQuery,
    exact_match: bool,
    show_hidden_features: bool,
    show_node_names: bool,
    preempt_jobs: Option<&PreemptJobs>,
    gpu: bool,
) -> TreeReportData {
    let mut root = TreeNode {
        name: "Total".to_string(),
        ..Default::default()
    };

    if selection.alternatives().len() == 1 {
        root.single_filter = true
    };

    // the main loop, iterating over the nodes in order to construct the tree structure
    for &node in nodes {
        let jobs_on_node = node_to_job_map
            .get(&node.id)
            .map(Vec::as_slice)
            .unwrap_or_default();

        // every level this node belongs to gets the same contribution
        let contribution = node_contribution(node, jobs, jobs_on_node, preempt_jobs, gpu);

        root.stats.add(&contribution);

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
        if selection.is_empty() {
            // by default, build tree from the (potentially filtered) feature list
            let mut current_level = &mut root;
            for feature in &features_for_tree {
                current_level = current_level
                    .children
                    .entry(feature.to_string())
                    .or_default();
                current_level.name = feature.to_string();
                current_level.stats.add(&contribution);

                if show_node_names {
                    current_level.stats.node_names.push(node.name.clone());
                }
            }
        } else {
            // bring what was selected to the top level, one branch per alternative, so that
            // a compound like icelake&gpu is one branch rather than one per feature in it
            for alternative in selection.alternatives() {
                // IMPORTANT: The check to see if a node belongs under a filter
                // must use the ORIGINAL, unfiltered features.
                if alternative.matches(node, exact_match) {
                    let label = alternative.label();
                    let mut current_level = root.children.entry(label.clone()).or_default();
                    current_level.name = label;
                    current_level.stats.add(&contribution);

                    // build the sub-branch from the *remaining* features,
                    // respecting the show_hidden_features flag
                    let named = alternative.features();
                    for feature in features_for_tree
                        .iter()
                        .filter(|f| !named.iter().any(|name| name == **f))
                    {
                        current_level = current_level
                            .children
                            .entry(feature.to_string())
                            .or_default();
                        current_level.name = feature.to_string();
                        current_level.stats.add(&contribution);

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

    let bars = count_blocks(width, percentage);

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

/// Bars are never drawn wider than this, however wide the terminal is
const BAR_WIDTH_MAX: usize = 20;
/// A bar narrower than this conveys nothing, so it is dropped instead
const BAR_WIDTH_MIN: usize = 4;

const TITLE_FEATURE: &str = "Feature";
const TITLE_NAMES: &str = "Node Names";
/// Availability column titles, as (long, short); the short form is used in narrow terminals
const TITLE_NODES: (&str, &str) = ("Nodes Available", "Nodes");
const TITLE_CORES: (&str, &str) = ("Cores Available", "Cores");
const TITLE_GPUS: (&str, &str) = ("GPUs Available", "GPUs");

/// The resolved column widths for one rendering of the tree table.
///
/// The header and data rows only line up if the title field is exactly `bar_width + 2` (the
/// width of a bar including its two `│`), so titles are chosen to fit that field and never
/// allowed to widen it.
struct Layout {
    feature_w: usize,
    nodes_w: usize,
    cpus_w: usize,
    /// `None` suppresses the bars, which moves the titles over the numeric columns
    bar_width: Option<usize>,
    /// Room reserved for the node-names title, or 0 when names are not shown. The hostlists
    /// themselves are unbounded and spill past the budget; only their title is laid out.
    names_w: usize,
}

impl Layout {
    /// Fits the table into `budget` columns by shrinking the bars. `budget` of `None` means
    /// unbounded, which yields full-width bars.
    ///
    /// The feature column is left at its natural width: if it alone overruns the budget the
    /// line is allowed to spill, since truncating feature names would hide the answer the
    /// user came for.
    fn solve(
        feature_w: usize,
        nodes_data_w: usize,
        cpus_data_w: usize,
        names_w: usize,
        gpu: bool,
        budget: Option<usize>,
    ) -> Self {
        let bar_width = match budget {
            None => BAR_WIDTH_MAX,
            Some(budget) => {
                // four single-space column gaps, plus the two `│` of each bar
                let fixed = feature_w + nodes_data_w + cpus_data_w + names_w + 4 + 4;
                BAR_WIDTH_MAX.min(budget.saturating_sub(fixed) / 2)
            }
        };

        if bar_width >= BAR_WIDTH_MIN {
            Layout {
                feature_w,
                nodes_w: nodes_data_w,
                cpus_w: cpus_data_w,
                bar_width: Some(bar_width),
                names_w,
            }
        } else {
            let cpus_title = if gpu { TITLE_GPUS } else { TITLE_CORES };
            Layout {
                feature_w,
                nodes_w: nodes_data_w.max(TITLE_NODES.1.len()),
                cpus_w: cpus_data_w.max(cpus_title.1.len()),
                bar_width: None,
                names_w,
            }
        }
    }

    /// Rendered width of one bar, including its two `│`
    fn bar_field(&self) -> usize {
        self.bar_width.map_or(0, |w| w + 2)
    }

    /// Width of a full row, which the separator line matches
    fn total_width(&self) -> usize {
        self.feature_w
            + self.nodes_w
            + self.cpus_w
            + 2
            + self.bar_width.map_or(0, |w| 2 * (w + 2) + 2)
    }

    /// The rule under the header, carrying a junction wherever a bar's `│` descends from it
    ///
    /// The names column has no fixed width, so the rule reaches only far enough to underline
    /// its title.
    fn separator(&self) -> String {
        let width = self.total_width() + self.names_w;
        let mut rule = vec!['═'; width];
        if let Some(w) = self.bar_width {
            let nodes_bar = self.feature_w + self.nodes_w + 2;
            let cpus_bar = nodes_bar + w + self.cpus_w + 4;
            for border in [nodes_bar, nodes_bar + w + 1, cpus_bar, cpus_bar + w + 1] {
                // the rightmost border ends the rule, so it takes a corner rather than a tee
                rule[border] = if border + 1 == width { '╕' } else { '╤' };
            }
        }
        rule.into_iter().collect()
    }

    /// The (nodes, cores/GPUs) titles, in their long form when the bars can hold it
    fn titles(&self, gpu: bool) -> (&'static str, &'static str) {
        let cpus = if gpu { TITLE_GPUS } else { TITLE_CORES };
        // a title starts at the bar's first cell, so it may run as far as the closing `│`;
        // both titles switch together so the two columns stay visually consistent
        match self.bar_width {
            Some(w) if TITLE_NODES.0.len().max(cpus.0.len()) <= w + 1 => (TITLE_NODES.0, cpus.0),
            _ => (TITLE_NODES.1, cpus.1),
        }
    }

    /// A bar prefixed by its column gap, or the empty string when bars are suppressed
    fn bar(&self, current: u32, total: u32, color: Color, no_color: bool) -> String {
        self.bar_width.map_or(String::new(), |w| {
            format!(" {}", create_avail_bar(current, total, w, color, no_color))
        })
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
///
/// `budget` is the target line width; `None` renders at full width.
pub fn print_tree_report(
    root: &TreeReportData,
    no_color: bool,
    show_node_names: bool,
    sort: bool,
    gpu: bool,
    budget: Option<usize>,
) {
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
    let feature_width = calculate_max_width(top_level_node, 0, false)
        .saturating_sub(4)
        .max(TITLE_FEATURE.len());

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

    let names_width = if show_node_names {
        TITLE_NAMES.len() + 1
    } else {
        0
    };
    let layout = Layout::solve(
        feature_width,
        nodes_data_width,
        cpus_data_width,
        names_width,
        gpu,
        budget,
    );

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
                let padding = " ".repeat(col_widths.max_preempt_nodes_width + 3);
                format!("{}{}/{}", idle_str, padding, total_str)
            } else {
                format!("{}/{}", idle_str, total_str)
            };
            (text.clone(), text)
        }
    };
    let nodes_width_adjusted = layout.nodes_w + node_text.len() - uncolored_node_text.len();

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
    let cpus_width_adjusted = layout.cpus_w + cpu_text.len() - uncolored_cpu_text.len();

    // getting the true max at the top level

    let max_nodes = stats.total_nodes;
    let max_cores = stats.total_cpus;

    let node_bar = layout.bar(stats.idle_nodes, stats.total_nodes, Color::Green, no_color);
    let cpu_bar = layout.bar(
        stats.idle_cpus,
        stats.total_cpus,
        if gpu { Color::Red } else { Color::Cyan },
        no_color,
    );

    // Print Headers with alignment
    let (nodes_title, cpus_title) = layout.titles(gpu);
    // the names column is unbounded, so its title trails the last column rather than
    // being padded into one
    let names_title = if show_node_names {
        format!(" {}", TITLE_NAMES.bold())
    } else {
        String::new()
    };
    if layout.bar_width.is_some() {
        // titles sit over the bars; the numeric columns are left unlabelled
        println!(
            "{:<feature_w$} {:<nodes_w$}  {:<bar_w$}{:<cpus_w$}  {:<cpus_title_w$}{}",
            TITLE_FEATURE.bold(),
            "",
            nodes_title.bold(),
            "",
            cpus_title.bold(),
            names_title,
            feature_w = layout.feature_w,
            nodes_w = layout.nodes_w,
            cpus_w = layout.cpus_w,
            bar_w = layout.bar_field(),
            // pad out to the bar's closing `│` so the names title lands in the gap after
            // it; the title fit rule guarantees cpus_title is no wider than that
            cpus_title_w = if show_node_names {
                layout.bar_field() - 1
            } else {
                0
            },
        );
    } else {
        println!(
            "{:<feature_w$} {:>nodes_w$} {:>cpus_w$}{}",
            TITLE_FEATURE.bold(),
            nodes_title.bold(),
            cpus_title.bold(),
            names_title,
            feature_w = layout.feature_w,
            nodes_w = layout.nodes_w,
            cpus_w = layout.cpus_w,
        );
    }

    // Print Separator Line
    println!("{}", layout.separator());

    // Print the top-level line using the adjusted widths for proper alignment
    println!(
        "{:<feature_w$} {:>nodes_w$}{} {:>cpus_w$}{}",
        top_level_node.name.bold(),
        node_text,
        node_bar,
        cpu_text,
        cpu_bar,
        feature_w = layout.feature_w,
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
            &layout,
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
    layout: &Layout,
    col_widths: &ColumnWidths,
    show_node_names: bool,
    sort: bool,
    max: (u32, u32),
    gpu: bool,
) {
    let mut path_parts = vec![tree_node.name.as_str()];
    let mut current_node = tree_node;

    while current_node.children.len() == 1 {
        let single_child = current_node.children.values().next().unwrap();
        if current_node.stats.total_nodes != single_child.stats.total_nodes {
            break;
        }
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
    let nodes_width_adjusted = layout.nodes_w + node_text.len() - uncolored_node_text.len();

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
    let cpus_width_adjusted = layout.cpus_w + cpu_text.len() - uncolored_cpu_text.len();

    let node_bar = layout.bar(stats.idle_nodes, max.0, Color::Green, no_color);
    let cpu_bar = layout.bar(
        stats.idle_cpus,
        max.1,
        if gpu { Color::Red } else { Color::Cyan },
        no_color,
    );

    let names = if show_node_names {
        format!(
            " {}",
            fi_slurm::parser::compress_hostlist(&current_node.stats.node_names)
        )
    } else {
        String::new()
    };

    println!(
        "{:<feature_w$} {:>nodes_w$}{} {:>cpus_w$}{}{}",
        display_name.bold(),
        node_text,
        node_bar,
        cpu_text,
        cpu_bar,
        names,
        feature_w = layout.feature_w,
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
            layout,
            col_widths,
            show_node_names,
            sort,
            (max.0, max.1),
            gpu,
        );
    }
}
