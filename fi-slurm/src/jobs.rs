use crate::parser::parse_tres_str;
use crate::states::{JobStateFlags, ShowFlags};
use crate::utils::{c_str_to_string, time_t_to_datetime};
use chrono::{DateTime, Utc};
use colored::Colorize;
use fi_slurm_sys::{job_info, job_info_msg_t, slurm_free_job_info_msg, slurm_load_jobs, time_t};
use std::collections::HashMap;
use std::ffi::CStr;

/// We use this struct to manage the C-allocated memory,
/// automatically dropping it when it goes out of memory
pub struct RawSlurmJobInfo {
    ptr: *mut job_info_msg_t,
}

impl Drop for RawSlurmJobInfo {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            // This unsafe block is necessary to call the FFI free function
            // We are confident it's safe because we're calling the paired `free`
            // function on a non-null pointer that we own
            unsafe {
                slurm_free_job_info_msg(self.ptr);
            }
        }
    }
}

impl RawSlurmJobInfo {
    /// Loads all job information from the Slurm controller.
    ///
    /// This is the only function that directly calls the unsafe `slurm_load_jobs`
    /// FFI function. On success, it returns an instance of the safe RAII wrapper,
    /// to be consumed by the .into_slurm_info() method
    pub fn load(update_time: time_t) -> Result<Self, String> {
        let mut job_info_msg_ptr: *mut job_info_msg_t = std::ptr::null_mut();

        // ALL so that jobs in hidden partitions are still counted
        let show_flags = ShowFlags::ALL | ShowFlags::DETAIL;

        let return_code =
            unsafe { slurm_load_jobs(update_time, &mut job_info_msg_ptr, show_flags.bits()) };

        if return_code == 0 && !job_info_msg_ptr.is_null() {
            // Success: wrap the raw pointer in our safe struct and return it.
            Ok(Self {
                ptr: job_info_msg_ptr,
            })
        } else {
            // Failure: return an error. No struct is created, no memory is leaked
            Err("Failed to load job information from Slurm".to_string())
        }
    }

    /// Provides safe, read-only access to the job data as a Rust slice
    ///
    /// This method is the bridge between the unsafe C array and safe, idiomatic
    /// Rust iterators. The returned slice is bounds-checked
    pub fn as_slice(&self) -> &[job_info] {
        if self.ptr.is_null() {
            return &[];
        }
        // This is `unsafe` because we are promising the compiler that the pointer
        // and record_count from the C library are valid
        unsafe {
            let msg = &*self.ptr;
            std::slice::from_raw_parts(msg.job_array, msg.record_count as usize)
        }
    }

    /// Consumes the wrapper to transform the raw C data into a safe, owned `SlurmJobs` collection
    pub fn into_slurm_jobs(self) -> Result<SlurmJobs, String> {
        let raw_jobs_slice = self.as_slice();

        let jobs_map: HashMap<JobId, Job> = raw_jobs_slice
            .iter()
            .map(|raw_job| {
                let job = Job::from_raw_binding(raw_job);
                (job.job_id, job)
            })
            .collect();

        let (last_update, last_backfill) = unsafe {
            let msg = &*self.ptr;
            (
                time_t_to_datetime(msg.last_update),
                time_t_to_datetime(msg.last_backfill),
            )
        };

        Ok(SlurmJobs {
            jobs: jobs_map,
            last_update,
            last_backfill,
        })
    }
}

/// Fetches all job information from Slurm and returns it as a safe,
/// owned Rust data structure
///
/// This function is the primary entry point for accessing job data. It handles
/// all unsafe FFI calls, data conversion, and memory management internally
pub fn get_jobs() -> Result<SlurmJobs, String> {
    // We load the raw C data into memory,
    // convert into safe, Rust-native structs,
    // and then consume the wrapper to drop the original C memory
    RawSlurmJobInfo::load(0)?.into_slurm_jobs()
}

/// Represents the state of a Slurm job in a type-safe way
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum JobState {
    Pending,
    Running,
    Suspended,
    Complete,
    Cancelled,
    Failed,
    Timeout,
    NodeFail,
    Preempted,
    BootFail,
    Deadline,
    OutOfMemory,
    End,
    /// A base state this build does not know, carrying the value Slurm reported
    Unknown(u32),
}

impl From<u32> for JobState {
    /// Slurm packs state flags above the base state, so the flag bits have to come off
    /// before the state can be recognized at all
    fn from(state_num: u32) -> Self {
        use fi_slurm_sys::*;

        match state_num & JOB_STATE_BASE {
            job_states_JOB_PENDING => JobState::Pending,
            job_states_JOB_RUNNING => JobState::Running,
            job_states_JOB_SUSPENDED => JobState::Suspended,
            job_states_JOB_COMPLETE => JobState::Complete,
            job_states_JOB_CANCELLED => JobState::Cancelled,
            job_states_JOB_FAILED => JobState::Failed,
            job_states_JOB_TIMEOUT => JobState::Timeout,
            job_states_JOB_NODE_FAIL => JobState::NodeFail,
            job_states_JOB_PREEMPTED => JobState::Preempted,
            job_states_JOB_BOOT_FAIL => JobState::BootFail,
            job_states_JOB_DEADLINE => JobState::Deadline,
            job_states_JOB_OOM => JobState::OutOfMemory,
            job_states_JOB_END => JobState::End,
            base => JobState::Unknown(base),
        }
    }
}

type JobId = u32;

/// A safe, owned, and idiomatic Rust representation of a Slurm job
///
/// This struct holds a curated subset of the most important fields from the
/// raw C `job_info` struct, converted into clean Rust types
/// We may expand these fields as we go in order to enable more features
#[derive(Debug, Clone)]
pub struct Job {
    // Core Identification
    pub job_id: JobId,
    pub array_job_id: u32,
    pub array_task_id: u32,
    pub name: String,
    pub user_id: u32,
    pub user_name: String,
    pub group_id: u32,
    pub partition: String,
    pub account: String,

    // State and Time
    pub job_state: JobState,
    /// The flags carried alongside `job_state`, e.g. a running job whose epilog has started
    pub state_flags: JobStateFlags,
    pub state_description: String,
    pub submit_time: DateTime<Utc>,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub time_limit_minutes: u32,
    pub preemptable_time: DateTime<Utc>,

    // Resource Allocation
    pub num_nodes: u32,
    pub num_cpus: u32,
    pub num_tasks: u32,
    pub raw_hostlist: String,
    pub node_ids: Vec<usize>,
    pub allocated_gres: HashMap<String, u64>,
    pub gres_total: Option<String>,

    // Other Information
    pub work_dir: String,
    pub command: String,
    pub exit_code: u32,
}

impl Job {
    /// Creates a safe, owned Rust `Job` from a raw C `job_info` struct
    pub fn from_raw_binding(raw_job: &job_info) -> Self {
        Job {
            job_id: raw_job.job_id,
            array_job_id: raw_job.array_job_id,
            array_task_id: raw_job.array_task_id,
            name: unsafe { c_str_to_string(raw_job.name) },
            user_id: raw_job.user_id,
            user_name: unsafe { c_str_to_string(raw_job.user_name) },
            group_id: raw_job.group_id,
            partition: unsafe { c_str_to_string(raw_job.partition) },
            account: unsafe { c_str_to_string(raw_job.account) },
            job_state: JobState::from(raw_job.job_state),
            state_flags: JobStateFlags::from_bits_truncate(raw_job.job_state),
            state_description: unsafe { c_str_to_string(raw_job.state_desc) },
            submit_time: time_t_to_datetime(raw_job.submit_time),
            start_time: time_t_to_datetime(raw_job.start_time),
            end_time: time_t_to_datetime(raw_job.end_time),
            time_limit_minutes: raw_job.time_limit,
            preemptable_time: time_t_to_datetime(raw_job.preemptable_time),
            num_nodes: raw_job.num_nodes,
            num_cpus: raw_job.num_cpus,
            num_tasks: raw_job.num_tasks,
            raw_hostlist: unsafe { c_str_to_string(raw_job.nodes) },
            node_ids: Vec::new(),
            allocated_gres: unsafe { parse_tres_str(raw_job.tres_alloc_str) },
            gres_total: if !raw_job.gres_total.is_null() {
                Some(
                    unsafe { CStr::from_ptr(raw_job.gres_total) }
                        .to_string_lossy()
                        .to_string(),
                )
            } else {
                None
            },
            // like the tres are
            work_dir: unsafe { c_str_to_string(raw_job.work_dir) },
            command: unsafe { c_str_to_string(raw_job.command) },
            exit_code: raw_job.exit_code,
        }
    }
}

pub enum FilterMethod {
    JobIds(Vec<u32>),
    UserId(u32),
    UserName(String),
    Partition(String),
    Account(String),
}

/// A safe, owned collection of Slurm jobs, mapping job ID to the Job object
#[derive(Debug, Clone)]
pub struct SlurmJobs {
    pub jobs: HashMap<u32, Job>,
    /// The timestamp of the last update from the Slurm controller
    pub last_update: DateTime<Utc>,
    /// Timestamp of the last backfill cycle, if available
    pub last_backfill: DateTime<Utc>,
}

impl SlurmJobs {
    pub fn filter_by(mut self, method: FilterMethod) -> Self {
        // go through the hashmap of jobs and figure out which ones either meet the user id
        // or the user name, just pass those back out, no need to change the other fields.
        self.jobs.retain(|_, job| match &method {
            FilterMethod::JobIds(ids) => ids.contains(&job.job_id),
            FilterMethod::UserId(id) => *id == job.user_id,
            FilterMethod::UserName(name) => *name == job.user_name,
            FilterMethod::Partition(partition) => *partition == job.partition,
            FilterMethod::Account(account) => *account == job.account,
        });

        Self {
            jobs: self.jobs,
            last_update: self.last_update,
            last_backfill: self.last_backfill,
        }
    }
    pub fn get_resource_use(&self) -> (u32, u32) {
        let (node_use, core_use) = self.jobs.iter().fold((0, 0), |mut acc, (_, job)| {
            acc.0 += job.num_nodes;
            acc.1 += job.num_cpus;
            acc
        });

        (node_use, core_use)
    }
    pub fn get_gres_total(&self) -> u32 {
        let gres_totals: Vec<Vec<u32>> = self
            .jobs
            .iter()
            .filter_map(|(_, job)| {
                if let Some(gres) = &job.gres_total {
                    let temp: Vec<u32> = gres
                        .split(':')
                        .filter_map(|g| g.parse::<u32>().ok())
                        .collect();
                    Some(temp)
                } else {
                    None
                }
            })
            .collect();

        // have to parse them out, to get the number after the last :

        gres_totals.iter().flatten().sum()
    }
}

/// Iterates through all loaded jobs and populates their `node_ids` vector.
/// This is a bulk operation designed for cache efficiency.
pub fn enrich_jobs_with_node_ids(
    slurm_jobs: &mut SlurmJobs, // Needs to be mutable to modify the jobs
    name_to_id: &HashMap<String, usize>,
) {
    // We iterate mutably over the jobs vector
    for job in &mut slurm_jobs.jobs.values_mut() {
        if job.raw_hostlist.is_empty() {
            continue;
        }

        // 1. Parse the hostlist string
        let expanded_nodes = crate::parser::parse_slurm_hostlist(&job.raw_hostlist);

        // 2. Convert names to IDs and populate the job's node_ids vector
        //    Pre-allocating capacity is a small extra optimization.
        job.node_ids.reserve(expanded_nodes.len());
        for node_name in expanded_nodes {
            if let Some(&id) = name_to_id.get(&node_name) {
                job.node_ids.push(id);
            }
        }

        // 3. (Optional) Free the memory from the raw string if it's no longer needed.
        job.raw_hostlist.clear();
        job.raw_hostlist.shrink_to_fit();
    }
}

/// Usage of one account, alongside the limits it counts against.
/// `None` is no limit, which is not the same as a limit of zero: Slurm reads a zero limit
/// as permission to run nothing at all.
#[derive(Clone)]
pub struct AccountJobUsage {
    pub account: String,
    /// Where these limits come from, shown in its own column when any row supplies it
    pub qos: Option<String>,
    pub cores: u32,
    pub nodes: u32,
    pub gpus: u32,
    pub jobs: u32,
    pub max_cores: Option<u32>,
    pub max_nodes: Option<u32>,
    pub max_gpus: Option<u32>,
    pub max_jobs: Option<u32>,
}

// to print a vector of account job usage in a sensible way

/// The column widths needed to print a set of `AccountJobUsage` rows. Widths can be
/// accumulated over several sets so that separate reports align with each other.
#[derive(Clone, Copy, Default)]
pub struct AcctUsageWidths {
    name_length: usize,
    qos_length: usize,
    core_length: usize,
    max_core_length: usize,
    node_length: usize,
    max_node_length: usize,
    gpu_length: usize,
    max_gpu_length: usize,
    job_length: usize,
    max_job_length: usize,
}

impl AcctUsageWidths {
    pub fn measure<'a>(mut self, accounts: impl IntoIterator<Item = &'a AccountJobUsage>) -> Self {
        for acc in accounts {
            self.name_length = self.name_length.max(acc.account.len());
            self.qos_length = self
                .qos_length
                .max(acc.qos.as_deref().map_or(0, |qos| qos.len()));
            self.core_length = self.core_length.max(acc.cores.to_string().len());
            self.max_core_length = self.max_core_length.max(limit_str(acc.max_cores).len());
            self.node_length = self.node_length.max(acc.nodes.to_string().len());
            self.max_node_length = self.max_node_length.max(limit_str(acc.max_nodes).len());
            self.gpu_length = self.gpu_length.max(acc.gpus.to_string().len());
            self.max_gpu_length = self.max_gpu_length.max(limit_str(acc.max_gpus).len());
            self.job_length = self.job_length.max(acc.jobs.to_string().len());
            self.max_job_length = self.max_job_length.max(limit_str(acc.max_jobs).len());
        }
        self
    }
}

fn limit_str(limit: Option<u32>) -> String {
    match limit {
        Some(n) => n.to_string(),
        None => "-".to_string(),
    }
}

/// The ends of the scale usage under its limit is drawn on: the first core, node or job
/// counted against the limit, and the approach to the limit itself. Usage that has reached
/// its limit leaves the scale for `OVER`.
///
/// Orange is not an ANSI color, so the scale comes from the truecolor range. `colored`
/// substitutes the nearest basic color where the terminal lacks truecolor support, which
/// flattens the scale to yellow but leaves `OVER` distinct from it.
const SCALE_LOW: (u8, u8, u8) = (255, 255, 0);
const SCALE_HIGH: (u8, u8, u8) = (255, 165, 0);
const OVER: (u8, u8, u8) = (255, 0, 0);

/// The color for usage `fraction` of the way to its limit, which the caller keeps under 1
fn usage_color(fraction: f64) -> (u8, u8, u8) {
    let fraction = fraction.clamp(0.0, 1.0);
    let mix = |low: u8, high: u8| {
        (f64::from(low) + fraction * (f64::from(high) - f64::from(low))).round() as u8
    };

    (
        mix(SCALE_LOW.0, SCALE_HIGH.0),
        mix(SCALE_LOW.1, SCALE_HIGH.1),
        mix(SCALE_LOW.2, SCALE_HIGH.2),
    )
}

/// Renders one "used/limit" cell, padded to `col_width` and colored by how much of the
/// limit is consumed. Only an idle cell is left uncolored.
fn usage_cell(
    used: u32,
    limit: Option<u32>,
    used_width: usize,
    limit_width: usize,
    col_width: usize,
) -> String {
    let plain = format!("{used:>used_width$}/{:>limit_width$}", limit_str(limit));
    // pad by the visible length, which the color escapes would otherwise inflate
    let pad = " ".repeat(col_width.saturating_sub(plain.len()));

    let color = match (used, limit) {
        (0, _) => None,
        // usage under no limit has nothing to be a fraction of, but is still worth seeing
        (_, None) => Some(SCALE_LOW),
        // a limit of zero permits nothing, which this arm also spares the division
        (used, Some(limit)) if used >= limit => Some(OVER),
        (used, Some(limit)) => Some(usage_color(f64::from(used) / f64::from(limit))),
    };

    let colored = match color {
        Some((r, g, b)) => plain.truecolor(r, g, b).to_string(),
        None => plain,
    };

    format!("{colored}{pad}")
}

/// One "used/limit" column: the widths of each half, and of the column as a whole
struct Column {
    used: usize,
    limit: usize,
    width: usize,
}

impl Column {
    fn new(used: usize, limit: usize, header: &str) -> Self {
        // the slash sits between the halves, and the header has to fit as well
        let width = (used + 1 + limit).max(header.len());
        Self { used, limit, width }
    }
}

/// A report's columns, laid out once so that printing it and measuring it cannot disagree
struct Layout {
    name: usize,
    /// `None` when no row carries a QOS, in which case the column is left out entirely
    qos: Option<usize>,
    cores: Column,
    nodes: Column,
    gpus: Column,
    jobs: Column,
}

const GAP: usize = 4;
const HEADER_QOS: &str = "QOS";
const HEADER_CORES: &str = "CORES";
const HEADER_NODES: &str = "NODES";
const HEADER_GPUS: &str = "GPUS";
const HEADER_JOBS: &str = "JOBS";

impl Layout {
    fn new(widths: &AcctUsageWidths, label_title: &str) -> Self {
        Self {
            // the title has to fit too, and every report sharing these widths is given the
            // same one, so they stay aligned
            name: widths.name_length.max(label_title.len()),
            qos: (widths.qos_length > 0).then(|| widths.qos_length.max(HEADER_QOS.len())),
            cores: Column::new(widths.core_length, widths.max_core_length, HEADER_CORES),
            nodes: Column::new(widths.node_length, widths.max_node_length, HEADER_NODES),
            gpus: Column::new(widths.gpu_length, widths.max_gpu_length, HEADER_GPUS),
            jobs: Column::new(widths.job_length, widths.max_job_length, HEADER_JOBS),
        }
    }

    /// The printed width of a line, which callers use to lay out headings over the report
    fn width(&self) -> usize {
        [
            Some(self.name),
            self.qos,
            Some(self.cores.width),
            Some(self.nodes.width),
            Some(self.gpus.width),
            Some(self.jobs.width),
        ]
        .into_iter()
        .flatten()
        .map(|column| column + GAP)
        .sum::<usize>()
            - GAP
    }

    fn header(&self, label_title: &str) -> String {
        let gap = " ".repeat(GAP);
        let name = self.name;
        let mut line = format!("{label_title:<name$}");
        if let Some(qos) = self.qos {
            line.push_str(&format!("{gap}{HEADER_QOS:<qos$}"));
        }
        let (cores, nodes, gpus, jobs) = (
            self.cores.width,
            self.nodes.width,
            self.gpus.width,
            self.jobs.width,
        );
        // the numeric headers are right-aligned over their columns
        line.push_str(&format!(
            "{gap}{HEADER_CORES:>cores$}{gap}{HEADER_NODES:>nodes$}{gap}{HEADER_GPUS:>gpus$}{gap}{HEADER_JOBS:>jobs$}"
        ));
        line
    }
}

impl AcctUsageWidths {
    /// The printed width of a report with these widths, for laying out a heading over it
    pub fn table_width(&self, label_title: &str) -> usize {
        Layout::new(self, label_title).width()
    }
}

/// `label_title` heads the first column, and must be the same for every report sharing
/// `widths` or they will not line up
pub fn print_accounts(accounts: &[AccountJobUsage], widths: &AcctUsageWidths, label_title: &str) {
    let layout = Layout::new(widths, label_title);
    let gap = " ".repeat(GAP);

    println!("{}", layout.header(label_title));

    for acc in accounts {
        // Each cell is padded to its column width, so the data lines up under its header
        let cells = [
            (acc.cores, acc.max_cores, &layout.cores),
            (acc.nodes, acc.max_nodes, &layout.nodes),
            (acc.gpus, acc.max_gpus, &layout.gpus),
            (acc.jobs, acc.max_jobs, &layout.jobs),
        ]
        .map(|(used, limit, column)| {
            usage_cell(used, limit, column.used, column.limit, column.width)
        });

        let name = layout.name;
        let mut line = format!("{:<name$}", acc.account);
        if let Some(width) = layout.qos {
            let qos = acc.qos.as_deref().unwrap_or("-");
            line.push_str(&format!("{gap}{qos:<width$}"));
        }
        for cell in &cells {
            line.push_str(&format!("{gap}{cell}"));
        }
        println!("{}", line);
    }
}

/// Builds a map where keys are node hostnames and values are a list of job IDs
/// running on that node
pub fn build_node_to_job_map(slurm_jobs: &SlurmJobs) -> HashMap<usize, Vec<u32>> {
    let mut node_to_job_map: HashMap<usize, Vec<u32>> = HashMap::new();

    for job in slurm_jobs.jobs.values() {
        if job.job_state != JobState::Running || job.node_ids.is_empty() {
            continue;
        }
        for &node_id in &job.node_ids {
            node_to_job_map.entry(node_id).or_default().push(job.job_id);
        }
    }
    node_to_job_map
}
