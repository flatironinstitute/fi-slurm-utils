use std::os::raw::c_void;
use rust_bind::bindings::{slurmdb_connection_close, slurmdb_connection_get};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbConnError {
    #[error("Could not establish connection to SlurmDB. Please ensure that SlurmDB is present and slurm_init has been run.")]
    DbConnectionError,
}

/// A Rust wrapper for a pointer to the SlurmDB database connection
pub struct DbConn {
    ptr: *mut c_void,
}

impl DbConn {
    /// Open a connection to the SlurmDB database, or else return an error
    pub fn new(persist_flags: &mut u16) -> Result<Self, DbConnError> {
        unsafe {
            let ptr = slurmdb_connection_get(persist_flags);

            if !ptr.is_null() {
                Ok(Self {
                    ptr
                })
            } else {
                Err(DbConnError::DbConnectionError)
            }
        }
    }
    
    /// Get a raw pointer to the SlurmDB connection
    pub fn as_mut_ptr(&mut self) -> *mut c_void { self.ptr }
}

impl Drop for DbConn {
    /// Close the SlurmDB connection safely using its destructor and null the pointer
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurmdb_connection_close(&mut self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

/// Wrapper function for the process of creating a DbConn
pub fn slurmdb_connect(persist_flags: &mut u16) -> Result<DbConn, DbConnError> {
    DbConn::new(persist_flags)
}
