use std::ffi::CStr;
use chrono::{DateTime, Utc};

pub fn time_t_to_datetime(timestamp: i64) -> DateTime<Utc> {
    chrono::DateTime::from_timestamp(timestamp, 0).unwrap_or_default()
}       
pub fn c_str_to_string(ptr: *const i8) -> String {
    if ptr.is_null() { String::new() }
    else { unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned() }
}
