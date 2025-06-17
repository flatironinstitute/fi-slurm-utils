use std::ffi::CStr;
use chrono::{DateTime, Utc};

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
