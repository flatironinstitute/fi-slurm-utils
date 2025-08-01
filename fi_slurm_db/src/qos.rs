use std::{
    ffi::{CStr, CString}, 
    ops::{Deref, DerefMut}, 
    os::raw::c_void
};
use chrono::{DateTime, Utc, Duration};

use rust_bind::bindings::{list_itr_t, slurm_list_append, slurm_list_create, slurm_list_destroy, slurm_list_iterator_create, slurm_list_iterator_destroy, slurm_list_next, slurmdb_assoc_cond_t, slurmdb_assoc_rec_t, slurmdb_connection_close, slurmdb_connection_get, slurmdb_qos_cond_t, slurmdb_qos_get, slurmdb_qos_rec_t, slurmdb_user_cond_t, slurmdb_user_rec_t, slurmdb_users_get, xlist};


use crate::db::DbConn;
use crate::utils::{vec_to_slurm_list, SlurmIterator};

use thiserror::Error;


#[derive(Error, Debug)]
pub enum QosError {
    #[error("Assoc vector was empty")]
    EmptyAssocError,
    #[error("No users found")]
    SlurmUserError,
    #[error("Pointer to assoc_list is null")]
    AssocListNull,
    #[error("Pointer to qos_list is null")]
    QosListNull,
    #[error("Pointer to user_list is null")]
    UserListNull,
    #[error("Database connection failed. Please ensure that SlurmDB is present and slurm_init has been run")]
    DbConnError,
    #[error("List of QoS successfully retrieved but empty")]
    EmptyQosListError,
}

pub struct QosConfig {
    pub name_list: Option<Vec<String>>,
    pub format_list: Option<Vec<String>>,
    pub id_list: Option<Vec<String>>,
    //...
    // refer to slurmdb_qos_cond_t in bindings for more fields
}

impl QosConfig {
    pub fn into_c_struct(self) -> slurmdb_qos_cond_t {
        unsafe {
            let mut c_struct: slurmdb_qos_cond_t = std::mem::zeroed();
            c_struct.name_list = vec_to_slurm_list(self.name_list);
            c_struct.format_list = vec_to_slurm_list(self.format_list);
            c_struct.id_list = vec_to_slurm_list(self.id_list);
            //...

            c_struct
        }
    }
}

/// Wrapper owning a heap-allocated Slurm QoS filter struct
pub struct QosQueryInfo {
    pub qos: *mut slurmdb_qos_cond_t,
}

impl QosQueryInfo {
    pub fn new(config: QosConfig) -> Self {
        // build zeroed C struct and heap-allocate so Slurm destroy frees heap
        let c_struct: slurmdb_qos_cond_t = config.into_c_struct();
        let boxed = Box::new(c_struct);
        let ptr = Box::into_raw(boxed);
        Self { qos: ptr }
    }
}

impl Drop for QosQueryInfo {
    fn drop(&mut self) {
        if !self.qos.is_null() {
            unsafe {
                // First, destroy the Slurm-allocated lists inside the struct
                let cond: &mut slurmdb_qos_cond_t = &mut *self.qos;

                if !cond.name_list.is_null() {
                    slurm_list_destroy(cond.name_list);
                }
                if !cond.format_list.is_null() {
                    slurm_list_destroy(cond.format_list);
                }
                if !cond.id_list.is_null() {
                    slurm_list_destroy(cond.id_list);
                }
                // add more lists here as we add them to the struct

                // Then, reconstruct the Box from the raw pointer. This gives
                // ownership back to Rust, which will correctly free the memory
                let _ = Box::from_raw(self.qos);
            }
            self.qos = std::ptr::null_mut();
        }
    }
}

impl Deref for QosQueryInfo {
    type Target = slurmdb_qos_cond_t;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.qos }
    }
}

pub struct SlurmQosList {
    pub ptr: *mut xlist,
}

impl SlurmQosList {
    pub fn new(db_conn: &mut DbConn, qos_query: &mut QosQueryInfo) -> Self {
        unsafe {
            // qos_query.qos is a *mut slurmdb_qos_cond_t
            let ptr = slurmdb_qos_get(db_conn.as_mut_ptr(), qos_query.qos);
            Self { ptr }
        }
    }
}

impl Drop for SlurmQosList {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurm_list_destroy(self.ptr);
            }
        }
    }
}

#[derive(Debug)]
pub struct SlurmQos {
    pub name: String,
    pub priority: u32,
    pub max_jobs_per_user: u32,
    pub max_tres_per_user: String,
    pub max_tres_per_account: String,
    pub max_tres_per_job: String,


    //...
    // refer to slurmdb_qos_rec_t in bindings
}

// might not have the relevant information, set on partition?
// bc not seeing 'scc' or 'other' qos in the output of sacctmgr ..., though cca does show up
// maybe the associations are the wrong data structure?
// maybe we need to look at partitions
// start working on the TRES part, that should be the same, at least

impl SlurmQos {
    pub unsafe fn from_c_rec(rec: *const slurmdb_qos_rec_t) -> Self {
        unsafe {
            // guard against null name pointer
            let name = if (*rec).name.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).name).to_string_lossy().into_owned()
            };

            let max_tres_per_user = if (*rec).max_tres_pu.is_null() {
                String::from("foo")
            } else {
                CStr::from_ptr((*rec).max_tres_pu).to_string_lossy().into_owned()
            };

            let max_tres_per_account = if (*rec).max_tres_pa.is_null() {
                String::from("foo")
            } else {
                CStr::from_ptr((*rec).max_tres_pa).to_string_lossy().into_owned()
            };

            let max_tres_per_job = if (*rec).max_tres_pj.is_null() {
                String::from("foo")
            } else {
                CStr::from_ptr((*rec).max_tres_pj).to_string_lossy().into_owned()
            };

            Self {
                name,
                priority: (*rec).priority,
                max_jobs_per_user: (*rec).max_jobs_pu,
                max_tres_per_user,
                max_tres_per_account,
                max_tres_per_job,
            }
        }
    }
}

pub fn process_qos_list(qos_list: SlurmQosList) -> Result<Vec<SlurmQos>, QosError> {

    if qos_list.ptr.is_null() {
        return Err(QosError::QosListNull)
    }

    let iterator = unsafe { SlurmIterator::new(qos_list.ptr) };

    let results: Vec<SlurmQos> = iterator.map(|node_ptr| {
        // not even an unsafe cast!
        let qos_rec_ptr = node_ptr as *const slurmdb_qos_rec_t;
        unsafe { SlurmQos::from_c_rec(qos_rec_ptr) }
    }).collect();

    if !results.is_empty() {
        Ok(results)
    } else {
        Err(QosError::EmptyQosListError)
    }
}


