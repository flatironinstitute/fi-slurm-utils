use chrono::{DateTime, Utc};
use std::collections::HashMap;
use crate::bindings::{job_info, job_info_msg_t, slurm_free_job_info_msg, slurm_load_jobs};
use crate::gres::parse_gres_str; 
use crate::utils::{c_str_to_string, time_t_to_datetime};

/// Fetches all job information from Slurm and returns it as a safe,
/// owned Rust data structure
///
/// This function is the primary entry point for accessing job data. It handles
/// all unsafe FFI calls, data conversion, and memory management internally
pub fn get_jobs() -> Result<SlurmJobs, String> {
    // We load the raw C data into memory,
    // convert into safe, Rust-native structs, 
    // and then consume the wrapper to drop the original C memory
    RawSlurmJobInfo::load()?.into_slurm_jobs()
}

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
    pub fn load() -> Result<Self, String> {
        let mut job_info_msg_ptr: *mut job_info_msg_t = std::ptr::null_mut();

        // TODO: The `show_flags` should be set with the appropriate constants
        // from the bindings file, e.g., `bindings::SHOW_ALL as u16`
        let show_flags = 0; // Placeholder for now

        let return_code = unsafe {
            slurm_load_jobs(0, &mut job_info_msg_ptr, show_flags)
        };

        if return_code == 0 && !job_info_msg_ptr.is_null() {
            // Success: wrap the raw pointer in our safe struct and return it.
            Ok(RawSlurmJobInfo { ptr: job_info_msg_ptr })
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

/// A safe, owned, and idiomatic Rust representation of a Slurm job
///
/// This struct holds a curated subset of the most important fields from the
/// raw C `job_info` struct, converted into clean Rust types
/// We may expand these fields as we go in order to enable more features
#[derive(Debug, Clone)]
pub struct Job {
    // Core Identification 
    pub job_id: u32,
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

    // Resource Allocation 
    pub num_nodes: u32,
    pub num_cpus: u32,
    pub num_tasks: u32,
    pub nodes: String,
    pub allocated_gres: HashMap<String, u64>,

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
             num_nodes: raw_job.num_nodes,
             num_cpus: raw_job.num_cpus,
             num_tasks: raw_job.num_tasks,
             nodes: unsafe {c_str_to_string(raw_job.nodes)},
             allocated_gres: unsafe {parse_gres_str(raw_job.tres_alloc_str)},
             work_dir: unsafe {c_str_to_string(raw_job.work_dir)},
             command: unsafe {c_str_to_string(raw_job.command)},
             exit_code: raw_job.exit_code,
        })
    }
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
