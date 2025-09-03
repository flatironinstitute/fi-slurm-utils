use std::collections::HashMap;
use std::ffi::CStr;
use std::sync::OnceLock;

use regex::Regex;

/// A robust parser for Slurm hostlist strings
///
/// This function can handle simple comma-separated lists as well as complex
/// ranged expressions with zero-padding
///
/// # Examples
///
/// * `"n01,n02"` -> `["n01", "n02"]`
/// * `"compute-b[10-12,15]"` -> `["compute-b10", "compute-b11", "compute-b12", "compute-b15"]`
/// * `"gpu-a[01-02]-ib"` -> `["gpu-a01-ib", "gpu-a02-ib"]`
///
/// # Arguments
///
/// * `hostlist_str` - A string slice representing the Slurm hostlist
///
/// # Returns
///
/// A `Vec<String>` containing all the individual, expanded hostnames
pub fn parse_slurm_hostlist(hostlist_str: &str) -> Vec<String> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(.*)\[([^\]]+)\](.*)$").expect("Failed to compile hostlist regex")
    });

    let mut expanded_nodes = Vec::new();
    let mut expressions = Vec::new();
    let mut current_expression = String::new();
    let mut bracket_level = 0;

    // This loop correctly separates expressions like "node[01-02],login01"
    // by respecting brackets
    for ch in hostlist_str.chars() {
        match ch {
            '[' => bracket_level += 1,
            ']' => bracket_level -= 1,
            ',' if bracket_level == 0 => {
                // We found a top-level comma separator.
                if !current_expression.is_empty() {
                    expressions.push(current_expression.trim().to_string());
                    current_expression.clear();
                }
                continue; // Skip adding the comma to the expression
            }
            _ => {}
        }
        current_expression.push(ch);
    }
    // Add the last expression to the list
    if !current_expression.is_empty() {
        expressions.push(current_expression.trim().to_string());
    }

    for part in expressions {
        // For each part, check if it matches our ranged expression regex
        if let Some(captures) = re.captures(&part) {
            // It's a ranged expression like "prefix[ranges]suffix"
            let prefix = captures.get(1).map_or("", |m| m.as_str());
            let range_list = captures.get(2).map_or("", |m| m.as_str());
            let suffix = captures.get(3).map_or("", |m| m.as_str());

            // Process the list of ranges inside the brackets
            for range_spec in range_list.split(',') {
                if let Some((start_str, end_str)) = range_spec.split_once('-') {
                    // It's a range like "01-03"
                    if let (Ok(start), Ok(end)) = (start_str.parse::<u32>(), end_str.parse::<u32>())
                        && start <= end {
                            // Detect zero-padding width from the start of the range
                            let width = start_str.len();
                            for i in start..=end {
                                // Format the number with leading zeros to match the width
                                expanded_nodes.push(format!(
                                    "{}{:0width$}{}",
                                    prefix,
                                    i,
                                    suffix,
                                    width = width
                                ));
                            }
                        }
                        // Ignore invalid ranges where start > end
                } else {
                    // It's a single number like "07"
                    expanded_nodes.push(format!("{}{}{}", prefix, range_spec, suffix));
                }
            }
        } else {
            // It's a simple hostname, not a ranged expression.
            if !part.is_empty() {
                expanded_nodes.push(part.to_string());
            }
        }
    }
    expanded_nodes
}

/// Compresses a vector of hostnames into a compact Slurm hostlist string.
/// This is the reverse operation of `parse_slurm_hostlist`.
pub fn compress_hostlist(nodes: &[String]) -> String {
    // Regex to parse a node name into three parts:
    // 1. A non-digit prefix (e.g., "node-")
    // 2. A numeric middle part (e.g., "007")
    // 3. A non-digit suffix (e.g., "-ib")
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| {
        Regex::new(r"^(\D*)(\d+)(\D*)$").expect("Failed to compile compression regex")
    });

    // A map to group nodes by their (prefix, suffix) pair.
    // The key is (prefix, suffix).
    // The value is a vector of (number, padding_width) tuples.
    let mut groups: HashMap<(String, String), Vec<(u32, usize)>> = HashMap::new();
    // A list to hold node names that don't fit the numeric pattern (e.g., "login").
    let mut standalone_nodes: Vec<String> = Vec::new();

    for node_name in nodes {
        if let Some(caps) = re.captures(node_name) {
            let prefix = caps.get(1).map_or("", |m| m.as_str()).to_string();
            let number_str = caps.get(2).map_or("", |m| m.as_str());
            let suffix = caps.get(3).map_or("", |m| m.as_str()).to_string();

            if let Ok(number) = number_str.parse::<u32>() {
                let padding = number_str.len();
                groups
                    .entry((prefix, suffix))
                    .or_default()
                    .push((number, padding));
            } else {
                standalone_nodes.push(node_name.clone());
            }
        } else {
            standalone_nodes.push(node_name.clone());
        }
    }

    // --- FIX: Re-classify single-node groups as standalone nodes ---
    // This prevents a single node like "login01" from being formatted as "login[01]".
    // We collect the keys of groups to modify first to avoid borrowing issues with the map.
    let single_node_group_keys: Vec<_> = groups
        .iter()
        .filter(|(_, numbers)| numbers.len() == 1)
        .map(|(key, _)| key.clone())
        .collect();

    for key in single_node_group_keys {
        if let Some(numbers) = groups.remove(&key) {
            let (prefix, suffix) = key;
            let (number, padding) = numbers[0];
            // Reconstruct the original node name and add it to the standalone list.
            // THIS LINE IS FIXED: Added `suffix` to the format arguments.
            let original_name = format!("{}{:0width$}{}", prefix, number, suffix, width = padding);
            standalone_nodes.push(original_name);
        }
    }
    // --- End of FIX ---

    let mut compressed_parts: Vec<String> = standalone_nodes;

    for ((prefix, suffix), mut numbers) in groups {
        // Sort numbers to make finding consecutive ranges easy.
        numbers.sort();

        if numbers.is_empty() {
            continue;
        }

        let mut ranges: Vec<String> = Vec::new();
        let mut current_start_num = numbers[0].0;
        let mut current_end_num = numbers[0].0;
        let mut current_padding = numbers[0].1;

        for &(num, padding) in &numbers[1..] {
            // A new range starts if the number is not consecutive OR if the padding changes.
            if num == current_end_num + 1 && padding == current_padding {
                current_end_num = num;
            } else {
                // Finalize the previous range
                if current_start_num == current_end_num {
                    ranges.push(format!(
                        "{:0width$}",
                        current_start_num,
                        width = current_padding
                    ));
                } else {
                    ranges.push(format!(
                        "{:0width$}-{:0width$}",
                        current_start_num,
                        current_end_num,
                        width = current_padding
                    ));
                }
                // Start a new range
                current_start_num = num;
                current_end_num = num;
                current_padding = padding;
            }
        }
        // Add the very last range
        if current_start_num == current_end_num {
            ranges.push(format!(
                "{:0width$}",
                current_start_num,
                width = current_padding
            ));
        } else {
            ranges.push(format!(
                "{:0width$}-{:0width$}",
                current_start_num,
                current_end_num,
                width = current_padding
            ));
        }

        // Only add brackets if there is something to put in them.
        if !ranges.is_empty() {
            compressed_parts.push(format!("{}[{}]{}", prefix, ranges.join(","), suffix));
        }
    }

    // Sort the final parts for a deterministic output order.
    compressed_parts.sort();
    compressed_parts.join(",")
}

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
                // find where the numeric part ends and the unit part begins, if it exists
                let numeric_end = value_str
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(value_str.len());

                let (numeric_part, unit_part) = value_str.split_at(numeric_end);

                if let Ok(base_value) = numeric_part.parse::<u64>() {
                    // if there is a string suffix, we try to parse that as a memory size indicator
                    // and multiply the count in order to produce byte counts
                    // assuming these refer to base-2 kibi- mebi- etc bytes
                    let multiplier = match unit_part.chars().next() {
                        Some('K') | Some('k') => 1024,
                        Some('M') | Some('m') => 1024 * 1024,
                        Some('G') | Some('g') => 1024 * 1024 * 1024,
                        Some('T') | Some('t') => 1024 * 1024 * 1024 * 1024,
                        _ => 1,
                    };
                    Some((key.to_string(), base_value * multiplier))
                } else {
                    None // Could not parse the numeric part
                }
            } else {
                None // Not a valid key=value pair
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_simple_list() {
        assert_eq!(
            parse_slurm_hostlist("n01,n02,n03"),
            vec!["n01", "n02", "n03"]
        );
    }

    #[test]
    fn test_single_node() {
        assert_eq!(parse_slurm_hostlist("login-a"), vec!["login-a"]);
    }

    #[test]
    fn test_simple_range() {
        assert_eq!(parse_slurm_hostlist("n[1-3]"), vec!["n1", "n2", "n3"]);
    }

    #[test]
    fn test_padded_range() {
        assert_eq!(parse_slurm_hostlist("n[01-03]"), vec!["n01", "n02", "n03"]);
        assert_eq!(
            parse_slurm_hostlist("gpu[007-009]"),
            vec!["gpu007", "gpu008", "gpu009"]
        );
    }

    #[test]
    #[should_panic]
    fn test_mixed_padded_range() {
        assert_eq!(parse_slurm_hostlist("n[01-03]"), vec!["n01", "n02", "n03"]);
        assert_eq!(
            parse_slurm_hostlist("gpu[07-009]"),
            vec!["gpu07", "gpu008", "gpu009"]
        );
        // in with multiple levels of padded range, the padding level of the first argument is used as default
    }

    #[test]
    fn test_mixed_padded_range_joined() {
        assert_eq!(
            parse_slurm_hostlist("n[001-003],gpu[07-09]"),
            vec!["n001", "n002", "n003", "gpu07", "gpu08", "gpu09"]
        );
        // in with multiple levels of padded range, the padding level of the first argument is used as default
    }

    #[test]
    fn test_mixed_padded_range_spaced() {
        assert_eq!(
            parse_slurm_hostlist("n[001-003], gpu[07-09]"),
            vec!["n001", "n002", "n003", "gpu07", "gpu08", "gpu09"]
        );
    }

    #[test]
    fn test_mixed_ranges() {
        assert_eq!(
            parse_slurm_hostlist("c[1,3-5,10]"),
            vec!["c1", "c3", "c4", "c5", "c10"]
        );
    }

    #[test]
    fn test_prefix_and_suffix() {
        assert_eq!(
            parse_slurm_hostlist("node-[08-10]-ib"),
            vec!["node-08-ib", "node-09-ib", "node-10-ib"]
        );
    }

    #[test]
    fn test_multiple_expressions() {
        assert_eq!(
            parse_slurm_hostlist("login01,node[01-02],gpu01"),
            vec!["login01", "node01", "node02", "gpu01"]
        );
    }

    #[test]
    fn test_compress_simple_consecutive() {
        let nodes = vec!["n01".to_string(), "n02".to_string(), "n03".to_string()];
        assert_eq!(compress_hostlist(&nodes), "n[01-03]");
    }

    #[test]
    fn test_compress_single_node() {
        let nodes = vec!["login01".to_string()];
        assert_eq!(compress_hostlist(&nodes), "login01");
    }

    #[test]
    fn test_compress_multiple_groups() {
        let nodes = vec![
            "n001".to_string(),
            "n002".to_string(),
            "n003".to_string(),
            "gpu07".to_string(),
            "gpu08".to_string(),
            "gpu09".to_string(),
        ];
        // Note: The output is sorted alphabetically by group for consistency.
        assert_eq!(compress_hostlist(&nodes), "gpu[07-09],n[001-003]");
    }

    #[test]
    fn test_compress_non_consecutive() {
        let nodes = vec![
            "c1".to_string(),
            "c3".to_string(),
            "c4".to_string(),
            "c5".to_string(),
            "c10".to_string(),
        ];
        assert_eq!(compress_hostlist(&nodes), "c[1,3-5,10]");
    }

    #[test]
    fn test_compress_with_standalone_nodes() {
        let nodes = vec!["n01".to_string(), "login".to_string(), "n02".to_string()];
        assert_eq!(compress_hostlist(&nodes), "login,n[01-02]");
    }

    #[test]
    fn test_compress_prefix_and_suffix() {
        let nodes = vec![
            "node-08-ib".to_string(),
            "node-09-ib".to_string(),
            "node-10-ib".to_string(),
        ];
        assert_eq!(compress_hostlist(&nodes), "node-[08-10]-ib");
    }

    #[test]
    fn test_compress_mixed_padding() {
        let nodes = vec![
            "n1".to_string(),
            "n2".to_string(),
            "n03".to_string(),
            "n04".to_string(),
        ];
        // The change in padding forces a new range.
        assert_eq!(compress_hostlist(&nodes), "n[1-2,03-04]");
    }

    #[test]
    fn test_simple_tres_string() {
        let input_str = "cpu=512,mem=4000G,node=4,billing=512";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(result_map.get("cpu"), Some(&512));
        assert_eq!(
            result_map.get("mem"),
            Some(4000 * 1024 * 1024 * 1024).as_ref()
        );
        assert_eq!(result_map.get("node"), Some(&4));
        assert_eq!(result_map.get("billing"), Some(&512));
    }

    #[test]
    fn test_tres_with_megabyte_suffix() {
        let input_str = "cpu=96,mem=1538000M,node=1,billing=96";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(result_map.get("cpu"), Some(&96));
        assert_eq!(result_map.get("mem"), Some(1538000 * 1024 * 1024).as_ref());
    }

    #[test]
    fn test_tres_with_gpu_gres() {
        let input_str = "cpu=32,mem=192G,node=1,billing=32,gres/gpu=8,gres/gpu:h100_pcie=8";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(
            result_map.get("mem"),
            Some(192 * 1024 * 1024 * 1024).as_ref()
        );
        assert_eq!(result_map.get("gres/gpu"), Some(&8));
        assert_eq!(result_map.get("gres/gpu:h100_pcie"), Some(&8));
        assert_eq!(result_map.get("cpu"), Some(&32));
    }
    #[test]
    fn test_tres_with_single_gpu_type() {
        // This tests a case where a specific GPU type might not be listed, only the generic one.
        let input_str = "cpu=10,mem=180T,node=1,billing=10,gres/gpu=1";
        let c_string = CString::new(input_str).unwrap();
        let result_map = unsafe { parse_tres_str(c_string.as_ptr()) };

        assert_eq!(
            result_map.get("mem"),
            Some(180 * 1024 * 1024 * 1024 * 1024).as_ref()
        );
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
