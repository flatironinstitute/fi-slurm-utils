use once_cell::sync::Lazy;
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
    // We use `once_cell::sync::Lazy` to compile the regex only once, the first
    // time it's needed. This is much more performant than compiling it on every call
    static RE: Lazy<Regex> = Lazy::new(|| {
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
        if let Some(captures) = RE.captures(&part) {
            // It's a ranged expression like "prefix[ranges]suffix"
            let prefix = captures.get(1).map_or("", |m| m.as_str());
            let range_list = captures.get(2).map_or("", |m| m.as_str());
            let suffix = captures.get(3).map_or("", |m| m.as_str());

            // Process the list of ranges inside the brackets
            for range_spec in range_list.split(',') {
                if let Some((start_str, end_str)) = range_spec.split_once('-') {
                    // It's a range like "01-03"
                    if let (Ok(start), Ok(end)) = (start_str.parse::<u32>(), end_str.parse::<u32>()) {
                        if start <= end {
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
                    }
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

// You can add unit tests within the same file to verify correctness.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_list() {
        assert_eq!(parse_slurm_hostlist("n01,n02,n03"), vec!["n01", "n02", "n03"]);
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
        assert_eq!(parse_slurm_hostlist("gpu[007-009]"), vec!["gpu007", "gpu008", "gpu009"]);
    }

    #[test]
    #[should_panic]
    fn test_mixed_padded_range() {
        assert_eq!(parse_slurm_hostlist("n[01-03]"), vec!["n01", "n02", "n03"]);
        assert_eq!(parse_slurm_hostlist("gpu[07-009]"), vec!["gpu07", "gpu008", "gpu009"]);
        // in with multiple levels of padded range, the padding level of the first argument is used as default
    }

    #[test]
    fn test_mixed_padded_range_joined() {
        assert_eq!(parse_slurm_hostlist("n[001-003],gpu[07-09]"), vec!["n001", "n002", "n003", "gpu07", "gpu08", "gpu09"]);
        // in with multiple levels of padded range, the padding level of the first argument is used as default
    }

    #[test]
    fn test_mixed_padded_range_spaced() {
        assert_eq!(parse_slurm_hostlist("n[001-003], gpu[07-09]"), vec!["n001", "n002", "n003", "gpu07", "gpu08", "gpu09"]);
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
}
