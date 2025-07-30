use std::os::raw::c_void;

use rust_bind::bindings::{slurmdb_connection_close, slurmdb_connection_get};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbConnError {
    #[error("Could not establish connection to SlurmDB. Please ensure that SlurmDB is present and slurm_init has been run.")]
    DbConnectionError,
}

pub struct DbConn {
    ptr: *mut c_void,
}

impl DbConn {
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
    
    pub fn as_mut_ptr(&mut self) -> *mut c_void { self.ptr }
}

impl Drop for DbConn {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            unsafe {
                slurmdb_connection_close(&mut self.ptr);
            }
            self.ptr = std::ptr::null_mut();
        }
    }
}

pub fn slurmdb_connect(persist_flags: &mut u16) -> Result<DbConn, DbConnError> {
    DbConn::new(persist_flags)
}
