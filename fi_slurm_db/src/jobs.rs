use std::{
    ffi::CStr, 
    ops::Deref, 
};

use chrono::{DateTime, Utc};
use rust_bind::bindings::{partition_info, slurm_list_destroy, slurmdb_job_cond_t, slurmdb_job_rec_t, slurmdb_jobs_get, xlist};


use crate::db::DbConn;
use crate::utils::{vec_to_slurm_list, SlurmIterator};

use thiserror::Error;


#[derive(Error, Debug)]
pub enum JobsError {
    #[error("Assoc vector was empty")]
    EmptyAssocError,
    #[error("No users found")]
    SlurmUserError,
    #[error("Pointer to assoc_list is null")]
    AssocListNull,
    #[error("Pointer to jobs_list is null")]
    JobsListNull,
    #[error("Pointer to user_list is null")]
    UserListNull,
    #[error("Database connection failed. Please ensure that SlurmDB is present and slurm_init has been run")]
    DbConnError,
    #[error("List of jobs successfully retrieved but empty")]
    EmptyJobsListError,
}

pub struct JobsConfig {
    pub acct_list: Option<Vec<String>>,
    pub format_list: Option<Vec<String>>,
    pub qos_list: Option<Vec<String>>,
    pub usage_end: DateTime<Utc>,
    pub usage_start: DateTime<Utc>,

    //...
    // refer to slurmdb_job_cond_t in bindings for more fields
}

impl JobsConfig {
    pub fn into_c_struct(self) -> slurmdb_job_cond_t {

        unsafe {
            let mut c_struct: slurmdb_job_cond_t = std::mem::zeroed();
            c_struct.acct_list = vec_to_slurm_list(self.acct_list);
            c_struct.format_list = vec_to_slurm_list(self.format_list);
            c_struct.qos_list = vec_to_slurm_list(self.qos_list);
            c_struct.usage_end = self.usage_end.timestamp();
            c_struct.usage_end = self.usage_start.timestamp();
            //...

            c_struct
        }
    }
}

/// Wrapper owning a heap-allocated Slurm jobs filter struct
pub struct JobsQueryInfo {
    pub jobs: *mut slurmdb_job_cond_t,
}

impl JobsQueryInfo {
    pub fn new(config: JobsConfig) -> Self {
        // build zeroed C struct and heap-allocate so Slurm destroy frees heap
        let c_struct: slurmdb_job_cond_t = config.into_c_struct();
        let boxed = Box::new(c_struct);
        let ptr = Box::into_raw(boxed);
        Self { jobs: ptr }
    }
}

impl Drop for JobsQueryInfo {
    fn drop(&mut self) {
        if !self.jobs.is_null() {
            unsafe {
                // First, destroy the Slurm-allocated lists inside the struct
                let cond: &mut slurmdb_job_cond_t = &mut *self.jobs;

                if !cond.acct_list.is_null() {
                    slurm_list_destroy(cond.acct_list);
                }
                if !cond.format_list.is_null() {
                    slurm_list_destroy(cond.format_list);
                }
                if !cond.qos_list.is_null() {
                    slurm_list_destroy(cond.qos_list);
                }
                // add more lists here as we add them to the struct

                // Then, reconstruct the Box from the raw pointer. This gives
                // ownership back to Rust, which will correctly free the memory
                let _ = Box::from_raw(self.jobs);
            }
            self.jobs = std::ptr::null_mut();
        }
    }
}

impl Deref for JobsQueryInfo {
    type Target = slurmdb_job_cond_t;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.jobs }
    }
}

pub struct SlurmJobsList {
    pub ptr: *mut xlist,
}

impl SlurmJobsList {
    pub fn new(mut db_conn: DbConn, jobs_query: &mut JobsQueryInfo) -> Self {
        unsafe {
            // jobs_query.jobs is a *mut slurmdb_jobs_cond_t
            let ptr = slurmdb_jobs_get(db_conn.as_mut_ptr(), jobs_query.jobs);
            Self { ptr }
        }
    }
}

impl Drop for SlurmJobsList {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_list_destroy(self.ptr);
            }
        }
    }
}

#[derive(Debug)]
pub struct SlurmJobs {
    pub job_id: u32,
    pub job_name: String,
    pub partition: String,
    pub priority: u32,
    pub node_names: String,
    pub alloc_nodes: u32,
    pub eligible: DateTime<Utc>,
    pub submit_time: DateTime<Utc>,


    //...
    // refer to slurmdb_job_rec_t in bindings
}

// might not have the relevant information, set on partition?
// bc not seeing 'scc' or 'other' jobs in the output of sacctmgr ..., though cca does show up
// maybe the associations are the wrong data structure?
// maybe we need to look at partitions
// start working on the TRES part, that should be the same, at least

impl SlurmJobs {
    pub unsafe fn from_c_rec(rec: *const slurmdb_job_rec_t) -> Self {
        unsafe {
            // guard against null name pointer



            let partition = if (*rec).partition.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).partition).to_string_lossy().into_owned()
            };

            let job_name = if (*rec).jobname.is_null() {
                String::from("foo")
            } else {
                CStr::from_ptr((*rec).jobname).to_string_lossy().into_owned()
            };


            //
            let node_names = if (*rec).nodes.is_null() {
                String::from("foo")
            } else {
                CStr::from_ptr((*rec).nodes).to_string_lossy().into_owned()
            };
            //
            // let max_tres_per_job = if (*rec).max_tres_pj.is_null() {
            //     String::from("foo")
            // } else {
            //     CStr::from_ptr((*rec).max_tres_pj).to_string_lossy().into_owned()
            // };

            Self {
                job_id: (*rec).jobid,
                job_name,
                partition,
                priority: (*rec).priority,
                node_names,
                alloc_nodes: (*rec).alloc_nodes,
                eligible: DateTime::from_timestamp((*rec).eligible, 0).unwrap(), // have to convert this i64 to datetime
                submit_time: DateTime::from_timestamp((*rec).submit, 0).unwrap(), // have to convert this i64 to datetime
            }
        }
    }
}

pub fn process_jobs_list(jobs_list: SlurmJobsList) -> Result<Vec<SlurmJobs>, JobsError> {

    if jobs_list.ptr.is_null() {
        return Err(JobsError::JobsListNull)
    }

    let iterator = unsafe { SlurmIterator::new(jobs_list.ptr) };

    let results: Vec<SlurmJobs> = iterator.map(|node_ptr| {
        // not even an unsafe cast!
        let jobs_rec_ptr = node_ptr as *const slurmdb_job_rec_t;
        unsafe { SlurmJobs::from_c_rec(jobs_rec_ptr) }
    }).collect();

    if !results.is_empty() {
        Ok(results)
    } else {
        Err(JobsError::EmptyJobsListError)
    }
}


