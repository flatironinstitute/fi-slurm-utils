use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::ffi::CStr;
use rust_bind::bindings::{job_info, job_info_msg_t, slurm_free_job_info_msg, slurm_load_jobs, time_t};
use crate::parser::parse_tres_str; 
use crate::utils::{c_str_to_string, time_t_to_datetime};

/// We use this struct to manage the C-allocatd memory,
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

        let show_flags = 2; // just using the SHOW_DETAIL flag

        let return_code = unsafe {
            slurm_load_jobs(update_time, &mut job_info_msg_ptr, show_flags)
        };

        if return_code == 0 && !job_info_msg_ptr.is_null() {
            // Success: wrap the raw pointer in our safe struct and return it.
            Ok(Self { ptr: job_info_msg_ptr })
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
    
    //pub fn into_message(self) -> JobInfoMsg {
    //    if self.ptr.is_null() {
    //        return 0 // create a more expressive return type, or error handling
    //
    //    }
    //
    //    unsafe {
    //        let msg = &*self.ptr;
    //        let last_backfill = msg.last_backfill;
    //        let last_update = msg.last_update;
    //        let record_count = msg.record_count;
    //        let job_array = msg.job_array;
    //
    //        JobInfoMsg {
    //            last_backfill,
    //            last_update,
    //            record_count,
    //            job_array
    //        }
    //    }
    //}
    /// Consumes the wrapper to transform the raw C data into a safe, owned `SlurmJobs` collection
    pub fn into_slurm_jobs(self) -> Result<SlurmJobs, String> {
        let raw_jobs_slice = self.as_slice();

        let jobs_map = raw_jobs_slice
            .iter()
            .try_fold(HashMap::new(), |mut map, raw_job| {
                let safe_job = Job::from_raw_binding(raw_job)?;
                map.insert(safe_job.job_id, safe_job);
                Ok::<HashMap<u32, Job>, String>(map)
            })?;
            
        let (last_update, last_backfill) = unsafe {
            let msg = &*self.ptr;
            (time_t_to_datetime(msg.last_update), time_t_to_datetime(msg.last_backfill))
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


struct _JobInfoMsg {
    last_backfill: time_t,
    last_update: time_t,
    record_count: u32,
    job_array: *mut job_info
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
    /// A catch-all for any state we don't explicitly handle.
    Unknown(String),
}

impl From<u32> for JobState {
    fn from(state_num: u32) -> Self {
        const JOB_PENDING: u32 = 0;
        const JOB_RUNNING: u32 = 1;
        const JOB_SUSPENDED: u32 = 2;
        const JOB_COMPLETE: u32 = 3; // Or COMPLETING
        const JOB_CANCELLED: u32 = 4;
        const JOB_FAILED: u32 = 5;
        const JOB_TIMEOUT: u32 = 6;
        const JOB_NODE_FAIL: u32 = 7;
        const JOB_PREEMPTED: u32 = 8;
        const JOB_BOOT_FAIL: u32 = 9;
        const JOB_DEADLINE: u32 = 10;
        const JOB_OUTOFMEMORY: u32 = 11;
        const JOB_END: u32 = 12;

        match state_num {
            JOB_PENDING => JobState::Pending,
            JOB_RUNNING => JobState::Running,
            JOB_SUSPENDED => JobState::Suspended,
            JOB_COMPLETE => JobState::Complete,
            JOB_CANCELLED => JobState::Cancelled,
            JOB_FAILED => JobState::Failed,
            JOB_TIMEOUT => JobState::Timeout,
            JOB_NODE_FAIL => JobState::NodeFail,
            JOB_PREEMPTED => JobState::Preempted,
            JOB_BOOT_FAIL => JobState::BootFail,
            JOB_DEADLINE => JobState::Deadline,
            JOB_OUTOFMEMORY => JobState::OutOfMemory,
            JOB_END => JobState::End,
            _ => JobState::Unknown(format!("State code {}", state_num)),
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
    pub fn from_raw_binding(raw_job: &job_info) -> Result<Self, String> {

        Ok(Job {
            job_id: raw_job.job_id,
            array_job_id: raw_job.array_job_id,
            array_task_id: raw_job.array_task_id,
            name: unsafe {c_str_to_string(raw_job.name)},
            user_id: raw_job.user_id,
            user_name: unsafe {c_str_to_string(raw_job.user_name)},
            group_id: raw_job.group_id,
            partition: unsafe {c_str_to_string(raw_job.partition)},
            account: unsafe {c_str_to_string(raw_job.account)},
            job_state: JobState::from(raw_job.job_state),
            state_description: unsafe {c_str_to_string(raw_job.state_desc)},
            submit_time: time_t_to_datetime(raw_job.submit_time),
            start_time: time_t_to_datetime(raw_job.start_time),
            end_time: time_t_to_datetime(raw_job.end_time),
            time_limit_minutes: raw_job.time_limit,
            preemptable_time: time_t_to_datetime(raw_job.preemptable_time),
            num_nodes: raw_job.num_nodes,
            num_cpus: raw_job.num_cpus,
            num_tasks: raw_job.num_tasks,
            raw_hostlist: unsafe {c_str_to_string(raw_job.nodes)},
            node_ids: Vec::new(),
            allocated_gres: unsafe {parse_tres_str(raw_job.tres_alloc_str)},
            gres_total: unsafe { if !raw_job.gres_total.is_null() { 
                Some(unsafe { CStr::from_ptr(raw_job.gres_total) }.to_string_lossy().to_string())
            } else { None }
            },
            // like the tres are 
            work_dir: unsafe {c_str_to_string(raw_job.work_dir)},
            command: unsafe {c_str_to_string(raw_job.command)},
            exit_code: raw_job.exit_code,
        })
    }
}

pub enum FilterMethod {
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
        self.jobs.retain(|_, job| { 
            match &method { 
                FilterMethod::UserId(id) => *id == job.user_id,
                FilterMethod::UserName(name) => *name == job.user_name,
                FilterMethod::Partition(partition) => *partition == job.partition,
                FilterMethod::Account(account) => *account == job.account,
            }
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
        let gres_totals: Vec<Option<String>> = self.jobs.iter().filter_map(|(_, job)| {
            if let Some(gres) = job.gres_total {
                gres.split(':').map(|g| {
                    if let Ok(count) = g.parse::<u32>() {
                        Some(count)
                    } else {
                        None
                    }
                })
            } else {
                None
            }
        }).collect();

        // have to parse them out, to get the number after the last :
        
        gres_totals.iter().sum()
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

pub struct AccountJobUsage {
    account: String,
    center_nodes: u32, 
    center_cores: u32, 
    center_gres: u32, 
    user_nodes: u32, 
    user_cores: u32, 
    user_gres: u32, 
    user_max_nodes: u32, 
    user_max_cores: u32,
    user_max_gres: u32,
    group_max_nodes: u32,
    group_max_cores: u32,
    group_max_gres: u32,
}

impl AccountJobUsage {
    pub fn new(
        account: &str, 
        center_nodes: u32, 
        center_cores: u32, 
        center_gres: u32, 
        user_nodes: u32, 
        user_cores: u32, 
        user_gres: u32, 
        user_max_nodes: u32, 
        user_max_cores: u32,
        user_max_gres: u32,
        group_max_nodes: u32,
        group_max_cores: u32,
        group_max_gres: u32,
    ) -> Self { 
        Self {
            account: account.to_string(),
            center_nodes, 
            center_cores, 
            center_gres, 
            user_nodes, 
            user_cores, 
            user_gres, 
            user_max_nodes, 
            user_max_cores,
            user_max_gres,
            group_max_nodes,
            group_max_cores,
            group_max_gres,
        }
    }
    pub fn print_user(&self, padding: usize) {
        println!("{} {} {}/{} {}/{} {}/{}", 
            self.account, 
            " ".repeat(padding), 
            self.user_cores, 
            self.user_max_cores,
            self.user_nodes, 
            self.user_max_nodes, 
            self.user_gres, 
            self.user_max_gres, 
        )
    }
    pub fn print_center(&self, padding: usize) {
        println!("{} {} {}/{} {}/{} {}/{}", 
            self.account, 
            " ".repeat(padding), 
            self.center_cores, 
            self.group_max_cores,
            self.center_nodes, 
            self.group_max_nodes, 
            self.center_gres, 
            self.group_max_gres, 
        )
    }
}
