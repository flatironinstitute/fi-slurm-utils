use std::ffi::CStr;
use chrono::{DateTime, Utc};
use rust_bind::bindings;

pub fn time_t_to_datetime(timestamp: i64) -> DateTime<Utc> {
    chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_default()
}       
/// Helper function turning a C String into an owned Rust String.
///
/// # Safety
///
/// This function may dereference a raw pointer. The caller must guarantee that the invoked pointer
/// is not null, out-of-bounds, or misaligned
pub unsafe fn c_str_to_string(ptr: *const i8) -> String {
    if ptr.is_null() { String::new() }
    else { unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned() }
}


// This struct ensures that the Slurm configuration is automatically
// loaded on creation and freed when it goes out of scope
pub struct SlurmConfig {
    _ptr: *mut bindings::slurm_conf_t,
}

impl SlurmConfig {
    /// Loads the Slurm configuration and returns a guard object
    /// The configuration will be freed when the guard is dropped
    pub fn load() -> Result<Self, String> {
        let mut conf_ptr: *mut bindings::slurm_conf_t = std::ptr::null_mut();
        unsafe {
            if bindings::slurm_load_ctl_conf(0, &mut conf_ptr) != 0 {
                return Err(
                    "Failed to load slurm.conf. Check logs or SLURM_CONF env variable."
                        .to_string(),
                );
            }
        }
        if conf_ptr.is_null() {
            // This is a defensive check; slurm_load_ctl_conf should not return 0
            // and a null pointer, but we check just in case
            return Err("slurm_load_ctl_conf succeeded but returned a null pointer.".to_string());
        }
        Ok(SlurmConfig { _ptr: conf_ptr })
    }
}

impl Drop for SlurmConfig {
    fn drop(&mut self) {
        // This is guaranteed to be called when the SlurmConfig instance
        // goes out of scope, preventing memory leaks
        unsafe {
            bindings::slurm_free_ctl_conf(self._ptr);
        }
        //println!("\nCleaned up Slurm configuration.");
    }
}

pub fn initialize_slurm() {
    unsafe {
        bindings::slurm_init(std::ptr::null());
    }
}
