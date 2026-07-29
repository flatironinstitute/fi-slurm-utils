use crate::nodes::{Node, SlurmNodes};
use std::collections::HashSet;

/// The characters Slurm reads as "and" in a feature expression: `&` between the terms of a
/// `--constraint`, `,` between the features a node declares.
const AND: [char; 2] = ['&', ','];
/// What Slurm reads as "or" in a `--constraint`
const OR: char = '|';
/// Slurm's counted and bracketed constraints exist to shape an allocation, so they mean
/// nothing when the job is to pick nodes to look at
const ALLOCATION_ONLY: [char; 5] = ['[', ']', '*', '(', ')'];

/// One way a node can satisfy a selection: a node must have every feature named here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alternative {
    features: Vec<String>,
}

impl Alternative {
    /// Whether `node` has all of these features, by substring unless `exact`
    pub fn matches(&self, node: &Node, exact: bool) -> bool {
        self.features.iter().all(|wanted| {
            if exact {
                node.features.contains(wanted)
            } else {
                node.features.iter().any(|held| held.contains(wanted))
            }
        })
    }

    /// How to name this alternative in a report, in the syntax it was written in
    pub fn label(&self) -> String {
        self.features.join("&")
    }

    /// The features named here, for callers that group by them
    pub fn features(&self) -> &[String] {
        &self.features
    }
}

/// Which nodes a report is about, as alternatives that a node may satisfy any of.
///
/// `icelake&gpu` and `icelake,gpu` both ask for nodes having both features. `icelake|gpu`
/// asks for either, as does naming them as separate arguments.
#[derive(Debug, Clone, Default)]
pub struct FeatureQuery {
    alternatives: Vec<Alternative>,
}

impl FeatureQuery {
    /// Reads a selection as written on a command line, one expression per argument
    pub fn parse(args: &[String]) -> Result<Self, String> {
        let mut alternatives = Vec::new();

        for arg in args {
            if let Some(bad) = arg.chars().find(|c| ALLOCATION_ONLY.contains(c)) {
                return Err(format!(
                    "'{bad}' in \"{arg}\" belongs to Slurm's counted and bracketed constraints, \
                     which shape an allocation and have no meaning when selecting nodes to \
                     display. Use & or , for and, | for or."
                ));
            }

            for alternative in arg.split(OR) {
                let features: Vec<String> = alternative
                    .split(AND)
                    .map(|feature| feature.trim().to_string())
                    .collect();

                if features.iter().any(String::is_empty) {
                    return Err(format!(
                        "\"{arg}\" has an operator with nothing on one side of it"
                    ));
                }

                alternatives.push(Alternative { features });
            }
        }

        Ok(Self { alternatives })
    }

    pub fn is_empty(&self) -> bool {
        self.alternatives.is_empty()
    }

    pub fn alternatives(&self) -> &[Alternative] {
        &self.alternatives
    }

    /// Whether `node` satisfies any alternative
    pub fn matches(&self, node: &Node, exact: bool) -> bool {
        self.alternatives
            .iter()
            .any(|alternative| alternative.matches(node, exact))
    }
}

/// Filters a collection of nodes by a feature selection.
///
/// This function is optimized to be very fast. It avoids cloning node data and
/// only performs the filtering logic if a selection is provided.
///
/// # Arguments
///
/// * `all_nodes` - A reference to the complete, unfiltered `SlurmNodes` collection.
/// * `selection` - The features to filter by.
/// * `exact_match` - A boolean to control matching behavior. If true, an exact match
///   is required. If false, substring matching is used.
///
/// # Returns
///
/// A `Vec` containing borrowed references to the nodes that passed the filter.
pub fn filter_nodes_by_feature<'a>(
    all_nodes: &'a SlurmNodes,
    selection: &FeatureQuery,
    exact_match: bool,
) -> Vec<&'a Node> {
    // Check if the selection is empty up front to select the most efficient path.
    if selection.is_empty() {
        // --- Optimized Path: No filters provided ---
        // Simply collect references to all nodes. This is a very cheap operation
        // with no cloning of Node data.
        all_nodes.nodes.iter().collect()
    } else {
        // --- Filtering Path: Filters were provided ---
        // Iterate through all nodes and collect references to only those that match.
        all_nodes
            .nodes
            .iter()
            .filter(|node| selection.matches(node, exact_match))
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
    for node in all_nodes.nodes.iter() {
        for feature in &node.features {
            all_features.insert(feature.clone());
        }
    }
    all_features
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> FeatureQuery {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        FeatureQuery::parse(&args).expect("should parse")
    }

    fn labels(query: &FeatureQuery) -> Vec<String> {
        query
            .alternatives()
            .iter()
            .map(Alternative::label)
            .collect()
    }

    #[test]
    fn ampersand_and_comma_both_mean_and() {
        assert_eq!(labels(&parse(&["icelake&gpu"])), ["icelake&gpu"]);
        assert_eq!(labels(&parse(&["icelake,gpu"])), ["icelake&gpu"]);
        assert_eq!(labels(&parse(&["icelake, gpu"])), ["icelake&gpu"]);
    }

    #[test]
    fn a_bar_and_separate_arguments_both_mean_or() {
        assert_eq!(labels(&parse(&["icelake|genoa"])), ["icelake", "genoa"]);
        assert_eq!(labels(&parse(&["icelake", "genoa"])), ["icelake", "genoa"]);
    }

    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(
            labels(&parse(&["icelake&gpu|genoa&gpu"])),
            ["icelake&gpu", "genoa&gpu"]
        );
    }

    #[test]
    fn no_arguments_selects_everything() {
        assert!(parse(&[]).is_empty());
    }

    #[test]
    fn allocation_only_syntax_is_refused() {
        for arg in ["[rack1|rack2]", "graphics*4", "(knl&hemi)"] {
            let err = FeatureQuery::parse(&[arg.to_string()]).expect_err("should be refused");
            assert!(err.contains("allocation"), "{arg}: {err}");
        }
    }

    #[test]
    fn a_dangling_operator_is_refused() {
        for arg in ["icelake&", "&icelake", "icelake&&gpu", "icelake,,gpu", "|"] {
            FeatureQuery::parse(&[arg.to_string()]).expect_err("should be refused");
        }
    }
}
