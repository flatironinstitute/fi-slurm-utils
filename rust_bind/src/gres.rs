use std::collections::HashMap;
use std::ffi::CStr;

/// Parses a raw, comma-separated GRES string from Slurm into a HashMap.
///
/// The function handles multiple GRES formats, such as "gpu:h100:4" or
/// "license:matlab:1". It is designed to be robust against malformed entries.
///
/// # Arguments
///
/// * `gres_ptr` - A raw C string pointer (`*const i8`) to the GRES string.
///
/// # Returns
///
/// A `HashMap<String, u64>` where the key is the GRES identifier (e.g., "gpu:h100")
/// and the value is the associated count.
pub unsafe fn parse_gres_str(gres_ptr: *const i8) -> HashMap<String, u64> {
    // If the pointer is null, there's nothing to parse. Return an empty map.
    if gres_ptr.is_null() {
        return HashMap::new();
    }

    // Safely convert the C string pointer to a Rust string slice.
    // .to_string_lossy() handles any potential invalid UTF-8 sequences.
    let gres_str = unsafe { CStr::from_ptr(gres_ptr) }.to_string_lossy();

    // If the resulting string is empty, we're done.
    if gres_str.is_empty() {
        return HashMap::new();
    }

    // The main parsing logic.
    gres_str
        .split(',') // 1. Split the full string into individual GRES definitions.
        .filter_map(|entry| {
            // 2. For each entry, split it into its component parts by the colon.
            let parts: Vec<&str> = entry.split(':').collect();

            match parts.as_slice() {
                // Case 1: Matches formats like "gpu:h100:4" or "name:type:count".
                [name, gres_type, count_str] => {
                    // The key combines the name and type for uniqueness.
                    let key = format!("{}:{}", name, gres_type);
                    // Parse the count string into a number.
                    let value = count_str.parse::<u64>().ok()?;
                    Some((key, value))
                }
                // Case 2: Matches formats like "license:2" or "name:count".
                [name, count_str] => {
                    let key = name.to_string();
                    let value = count_str.parse::<u64>().ok()?;
                    Some((key, value))
                }
                // If the format is unexpected, filter_map with `None` will discard it.
                _ => None,
            }
        })
        .collect() // 3. Collect the valid (key, value) pairs into a HashMap.
}

