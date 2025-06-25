use std::collections::HashMap;
use std::ffi::CStr;

/// Parses a raw, comma-separated GRES string from Slurm into a HashMap.
///
/// This function is designed to be robust against complex GRES strings, such as
/// "gpu:a100:4(IDX:0-3)", by correctly identifying the full GRES name and
/// extracting only the numerical part of the count.
///
/// # Arguments
///
/// * `gres_ptr` - A raw C string pointer (`*const i8`) to the GRES string.
///
/// # Returns
///
/// A `HashMap<String, u64>` where the key is the full GRES identifier
/// (e.g., "gpu:a100-sxm4-40gb") and the value is the associated count
#[allow(clippy::missing_safety_doc)]
pub unsafe fn parse_gres_str(gres_ptr: *const i8) -> HashMap<String, u64> {
    // If the pointer is null, there's nothing to parse. Return an empty map
    if gres_ptr.is_null() {
        return HashMap::new();
    }

    // Safely convert the C string pointer to a Rust string slice
    let gres_str = unsafe { CStr::from_ptr(gres_ptr) }.to_string_lossy();

    if gres_str.is_empty() {
        return HashMap::new();
    }

    // The new, more robust parsing logic
    gres_str
        .split(',') // 1. Split the full string into individual GRES definitions
        .filter_map(|entry| {
            // 2. Find the last colon to separate the name from the count part
            // `rsplit_once` is perfect for this
            if let Some((name, count_part)) = entry.rsplit_once(':') {
                // `name` is now the full key, e.g., "gpu:a100-sxm4-40gb"
                // `count_part` is the rest, e.g., "4(IDX:0-3)"

                // 3. Extract only the leading digits from the count part
                let count_str: String = count_part
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();

                // 4. Parse the extracted digits into a number
                if let Ok(value) = count_str.parse::<u64>() {
                    Some((name.to_string(), value))
                } else {
                    None // Discard if parsing fails
                }
            } else {
                None // Discard entries that don't have a colon
            }
        })
        .collect() // 5. Collect the valid (key, value) pairs into a HashMap
}
