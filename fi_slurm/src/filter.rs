use crate::nodes::{Node, SlurmNodes};
use crate::compound_filter::{evaluate, FeatureExpression};
use std::collections::HashSet;

/// Filters a collection of nodes based on a parsed `FeatureExpression` AST.
///
/// If no expression is provided (`filter_ast` is None), it returns references
/// to all nodes. Otherwise, it evaluates the expression for each node.
///
/// # Arguments
///
/// * `all_nodes` - A reference to the complete, unfiltered `SlurmNodes` collection.
/// * `filter_ast` - An `Option` containing the parsed expression tree.
/// * `exact_match` - A boolean to control matching behavior for terms in the expression.
///
/// # Returns
///
/// A `Vec` containing borrowed references to the nodes that passed the filter.
pub fn filter_nodes_by_expression<'a>(
    all_nodes: &'a SlurmNodes,
    filter_ast: &Option<FeatureExpression>,
    exact_match: bool,
) -> Vec<&'a Node> {
    let Some(expr) = filter_ast else {
        // If no filter expression is provided, return all nodes.
        return all_nodes.nodes.values().collect();
    };

    // If there is an expression, filter the nodes by evaluating it.
    all_nodes
        .nodes
        .values()
        .filter(|node| evaluate(expr, node, exact_match))
        .collect()
}


/// Filters a collection of nodes based on a list of required features.
///
/// This function is optimized to be very fast. It avoids cloning node data and
/// only performs the filtering logic if a filter is provided.
///
/// # Arguments
///
/// * `all_nodes` - A reference to the complete, unfiltered `SlurmNodes` collection.
/// * `feature_filter` - A slice of strings representing the features to filter by.
/// * `exact_match` - A boolean to control matching behavior. If true, an exact match
///                   is required. If false, substring matching is used.
///
/// # Returns
///
/// A `Vec` containing borrowed references to the nodes that passed the filter.
pub fn filter_nodes_by_feature<'a>(
    all_nodes: &'a SlurmNodes,
    feature_filter: &[String],
    exact_match: bool,
) -> Vec<&'a Node> {
    // Check if the filter is empty up front to select the most efficient path.
    if feature_filter.is_empty() {
        // --- Optimized Path: No filters provided ---
        // Simply collect references to all nodes. This is a very cheap operation
        // with no cloning of Node data.
        all_nodes.nodes.values().collect()
    } else {
        // --- Filtering Path: Filters were provided ---
        // Iterate through all nodes and collect references to only those that match.
        all_nodes
            .nodes
            .values()
            .filter(|node| {
                // The `any` closure returns true if the node should be included.
                feature_filter.iter().any(|required_feat| {
                    if exact_match {
                        // Exact matching
                        node.features.contains(required_feat)
                    } else {
                        // Substring matching
                        node.features
                            .iter()
                            .any(|actual_feat| actual_feat.contains(required_feat))
                    }
                })
            })
            .collect()
    }
}

/// Gathers a complete set of all unique features available on the cluster.
///
/// This is a relatively expensive operation as it iterates through every feature
/// on every node and clones the string data. It should only be called when needed,
/// for example, to provide helpful error messages to the user.
///
/// # Arguments
///
/// * `all_nodes` - A reference to the complete `SlurmNodes` collection.
///
/// # Returns
///
/// A `HashSet<String>` containing all unique feature names.
pub fn gather_all_features(all_nodes: &SlurmNodes) -> HashSet<String> {
    let mut all_features = HashSet::new();
    for node in all_nodes.nodes.values() {
        for feature in &node.features {
            all_features.insert(feature.clone());
        }
    }
    all_features
}
