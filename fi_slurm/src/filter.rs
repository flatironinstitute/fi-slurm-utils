use crate::nodes::{Node, SlurmNodes};
use std::collections::{HashMap, HashSet};

/// A struct to hold the results of the filtering operation.
///
/// It uses a lifetime `'a` to indicate that the `nodes` vector contains
/// borrows of `Node` objects that live elsewhere (in the original `SlurmNodes`).
pub struct FilteredNodes<'a> {
    /// A vector of references to the nodes that passed the filter.
    pub nodes: Vec<&'a Node>,
    /// A set of all unique features found across all nodes (pre-filtering).
    pub all_features: HashSet<String>,
}

/// Filters a collection of nodes based on a list of required features.
///
/// This function serves as the primary interface for filtering. It also gathers
/// a set of all unique features available on the cluster, which can be used to
/// validate user input.
///
/// # Arguments
///
/// * `all_nodes` - A reference to the complete, unfiltered `SlurmNodes` collection.
/// * `feature_filter` - A slice of strings representing the features to filter by.
///                      If empty, all nodes are returned.
/// * `exact_match` - A boolean to control matching behavior. If true, an exact match
///                   is required. If false (default), substring matching is used.
///
/// # Returns
///
/// A `FilteredNodes` struct containing a vector of borrowed references to the
/// filtered nodes, and the set of all unique features.
pub fn filter_nodes_by_feature<'a>(
    all_nodes: &'a SlurmNodes,
    feature_filter: &[String],
    exact_match: bool,
) -> FilteredNodes<'a> {
    let mut all_features = HashSet::new();
    let mut filtered_nodes_vec = Vec::new();

    // --- First, iterate to gather all features and perform filtering ---
    for node in all_nodes.nodes.values() {
        // Collect all unique features from every node.
        for feature in &node.features {
            all_features.insert(feature.clone());
        }

        // --- Filtering Logic ---
        let should_include = if feature_filter.is_empty() {
            // If no filter is provided, include every node.
            true
        } else {
            // Otherwise, check if the node has at least one of the required features,
            // using the matching strategy specified by the `exact_match` flag.
            feature_filter.iter().any(|required_feat| {
                if exact_match {
                    // Exact matching: The node's feature list must contain the exact string.
                    node.features.contains(required_feat)
                } else {
                    // Substring matching (default): Check if any of the node's actual
                    // features contain the user's filter string.
                    node.features
                        .iter()
                        .any(|actual_feat| actual_feat.contains(required_feat))
                }
            })
        };

        if should_include {
            // Push a reference to the node that passed the filter.
            filtered_nodes_vec.push(node);
        }
    }

    // Construct the final result.
    FilteredNodes {
        nodes: filtered_nodes_vec,
        all_features,
    }
}
