use chrono::{DateTime, Utc};
use rust_bind::bindings;
use std::ffi::CStr;

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
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    }
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
                    "Failed to load slurm.conf. Check logs or SLURM_CONF env variable.".to_string(),
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

pub fn count_blocks(max_blocks: usize, percentage: f64) -> (usize, usize, Option<String>) {
    // Use floating point numbers for precision and round at the end
    // to get the closest visual representation
    let total_segments = max_blocks as f64 * 8.0;
    let filled_segments = (total_segments * percentage).round() as usize;

    // The number of full blocks is the integer division of filled segments
    let full_blocks = filled_segments / 8;

    // The remainder determines the partial block character
    let remainder_segments = filled_segments % 8;

    let partial_block = match remainder_segments {
        1 => Some("▏".to_string()),
        2 => Some("▎".to_string()),
        3 => Some("▍".to_string()),
        4 => Some("▌".to_string()),
        5 => Some("▋".to_string()),
        6 => Some("▊".to_string()),
        7 => Some("▉".to_string()),
        _ => None, // This covers the case where remainder_segments is 0
    };

    // The number of empty blocks is what's left over to reach max_blocks
    let partial_block_count = if remainder_segments > 0 { 1 } else { 0 };
    let empty_blocks = max_blocks.saturating_sub(full_blocks + partial_block_count);

    (full_blocks, empty_blocks, partial_block)
}

#[cfg(test)]
pub mod tests {
    use super::count_blocks;

    #[test]
    fn t1() {
        let result = count_blocks(20, 0.95);
        assert_eq!(result.0, 19);
        assert_eq!(result.1, 1);
        assert_eq!(result.2, None);
    }
    #[test]
    fn t2() {
        let result = count_blocks(20, 0.92);
        assert_eq!(result.0, 18);
        assert_eq!(result.1, 1);
        assert_eq!(result.2, Some("▍".to_string()));
    }
}
