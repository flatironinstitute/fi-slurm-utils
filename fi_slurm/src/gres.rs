use std::collections::HashMap;
use std::ffi::CStr;

/// Parses a comma-separated TRES string (e.g., "cpu=4,mem=8G,gres/gpu=1")
/// from a job's resource allocation into a HashMap.
///
/// This function correctly handles numeric values with suffixes like 'M' and 'G'
/// by parsing only the leading digits.
///
/// # Arguments
///
/// * `tres_ptr` - A raw C string pointer (`*const i8`) to the TRES string.
///
/// # Returns
///
/// A `HashMap<String, u64>` where the key is the resource name (e.g., "mem", "gres/gpu")
/// and the value is the associated count.
#[allow(clippy::missing_safety_doc)]
pub unsafe fn parse_tres_str(tres_ptr: *const i8) -> HashMap<String, u64> {
    if tres_ptr.is_null() {
        return HashMap::new();
    }

    let tres_str = unsafe { CStr::from_ptr(tres_ptr) }.to_string_lossy();

    if tres_str.is_empty() {
        return HashMap::new();
    }

    tres_str
        .split(',')
        .filter_map(|pair| {
            // Split each part by the '=' to get the key and value.
            if let Some((key, value_str)) = pair.split_once('=') {
                // For the value, take only the leading digits and ignore
                // any suffixes like 'M', 'G', etc.
                let numeric_part: String = value_str
                    .chars()
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                
                if let Ok(value) = numeric_part.parse::<u64>() {
                    Some((key.to_string(), value))
                } else {
                    None // Could not parse the numeric part
                }
            } else {
                None // Not a valid key=value pair
            }
        })
        .collect()
}

// This module ensures our parser works correctly with real-world data
#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_simple_tres_string() {
        let input_str = "cpu=512,mem=4000G,node=4,billing=512";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(result_map.get("cpu"), Some(&512));
        assert_eq!(result_map.get("mem"), Some(&4000));
        assert_eq!(result_map.get("node"), Some(&4));
        assert_eq!(result_map.get("billing"), Some(&512));
    }

    #[test]
    fn test_tres_with_megabyte_suffix() {
        let input_str = "cpu=96,mem=1538000M,node=1,billing=96";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(result_map.get("cpu"), Some(&96));
        assert_eq!(result_map.get("mem"), Some(&1538000));
    }

    #[test]
    fn test_tres_with_gpu_gres() {
        let input_str = "cpu=32,mem=192G,node=1,billing=32,gres/gpu=8,gres/gpu:h100_pcie=8";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(result_map.get("gres/gpu"), Some(&8));
        assert_eq!(result_map.get("gres/gpu:h100_pcie"), Some(&8));
        assert_eq!(result_map.get("cpu"), Some(&32));
    }

    #[test]
    fn test_tres_with_single_gpu_type() {
        // This tests a case where a specific GPU type might not be listed, only the generic one.
        let input_str = "cpu=10,mem=180G,node=1,billing=10,gres/gpu=1";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(result_map.get("gres/gpu"), Some(&1));
        assert_eq!(result_map.get("gres/gpu:a100-sxm4-40gb"), None); // Ensure it doesn't exist
    }

    #[test]
    fn test_empty_string() {
        let input_str = "";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };
        assert!(result_map.is_empty());
    }
}
