use fi_slurm_sys::{
    slurm_list_destroy, slurmdb_qos_cond_t, slurmdb_qos_get, slurmdb_qos_rec_t, xlist,
};
use std::{ffi::CStr, ops::Deref};
use thiserror::Error;

use crate::db::DbConn;
use crate::utils::{SlurmIterator, vec_to_slurm_list};

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
    #[error(
        "Database connection failed. Please ensure that SlurmDB is present and slurm_init has been run"
    )]
    DbConnError,
    #[error("List of QoS successfully retrieved but empty")]
    EmptyQosListError,
    #[error("Failed to load partitions: {0}")]
    PartitionLoadError(String),
}

/// A Rust-side object corresponding to the slurmdb_qos_cond_t object
pub struct QosConfig {
    pub name_list: Option<Vec<String>>,
    pub format_list: Option<Vec<String>>,
    pub id_list: Option<Vec<String>>,
}

impl QosConfig {
    /// Converting a QosConfig object into a slurmdb_qos_cond_t object to be passed into Slurm
    pub fn into_c_struct(self) -> slurmdb_qos_cond_t {
        unsafe {
            let mut c_struct: slurmdb_qos_cond_t = std::mem::zeroed();
            c_struct.name_list = vec_to_slurm_list(self.name_list);
            c_struct.format_list = vec_to_slurm_list(self.format_list);
            c_struct.id_list = vec_to_slurm_list(self.id_list);
            //... add more fields as needed

            c_struct
        }
    }
}

/// Wrapper owning a heap-allocated Slurm QoS filter struct
pub struct QosQueryInfo {
    pub qos: *mut slurmdb_qos_cond_t,
}

impl QosQueryInfo {
    /// Constructing a QosQueryInfo wrapper object from a pointer to a pointer to a C struct
    pub fn new(config: QosConfig) -> Self {
        // build zeroed C struct and heap-allocate so Slurm destroy frees heap
        let c_struct: slurmdb_qos_cond_t = config.into_c_struct();
        let boxed = Box::new(c_struct);
        let ptr = Box::into_raw(boxed);
        Self { qos: ptr }
    }
}

impl Drop for QosQueryInfo {
    /// Safely destroy the Slurm-allocated lists in the QosQueryInfo struct
    /// We free the individual lists with their destructor functions,
    /// and then, by creating a Rust Box from the top-level pointer, we
    /// claim the memory from C, and Rust safely drops it at the end of scope
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
/// A Rust object holding part of the information from a slurmdb_qos_rec_t object.
/// An unset limit is `None`, or `u32::MAX` (Slurm's INFINITE) for the scalar ones.
pub struct SlurmQos {
    pub name: String,
    pub priority: u32,
    pub max_jobs_per_user: u32,
    pub max_tres_per_user: Option<String>,
    pub max_tres_per_group: Option<String>,
    pub max_tres_per_account: Option<String>,
    pub max_tres_per_job: Option<String>,
}

impl SlurmQos {
    /// Generate a SlurmQos object from a C slurmdb_qos_rec_t object
    /// # Safety
    /// This function is unsafe because it dereferences a raw pointer from C.
    /// The caller must ensure that the pointer is valid and points to a properly initialized
    /// slurmdb_qos_rec_t struct.
    pub unsafe fn from_c_rec(rec: *const slurmdb_qos_rec_t) -> Self {
        unsafe {
            // guard against null name pointer
            let name = if (*rec).name.is_null() {
                String::new()
            } else {
                CStr::from_ptr((*rec).name).to_string_lossy().into_owned()
            };

            Self {
                name,
                priority: (*rec).priority,
                max_jobs_per_user: (*rec).max_jobs_pu,
                max_tres_per_user: tres_str((*rec).max_tres_pu),
                max_tres_per_group: tres_str((*rec).grp_tres),
                max_tres_per_account: tres_str((*rec).max_tres_pa),
                max_tres_per_job: tres_str((*rec).max_tres_pj),
            }
        }
    }
}

/// Reads an optional TRES limit string, which Slurm leaves null when unset
/// # Safety
/// The caller must ensure `ptr` is either null or a valid C string.
unsafe fn tres_str(ptr: *const std::os::raw::c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// Process a SlurmQosList into a vector of SlurmQos objects, or else return an Error
pub fn process_qos_list(qos_list: SlurmQosList) -> Result<Vec<SlurmQos>, QosError> {
    if qos_list.ptr.is_null() {
        return Err(QosError::QosListNull);
    }

    let iterator = unsafe { SlurmIterator::new(qos_list.ptr) };

    let results: Vec<SlurmQos> = iterator
        .map(|node_ptr| {
            // not even an unsafe cast!
            let qos_rec_ptr = node_ptr as *const slurmdb_qos_rec_t;
            unsafe { SlurmQos::from_c_rec(qos_rec_ptr) }
        })
        .collect();

    if !results.is_empty() {
        Ok(results)
    } else {
        Err(QosError::EmptyQosListError)
    }
}
