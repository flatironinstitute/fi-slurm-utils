#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

pub const AUTHOR_HELP: &str = "Maintainer: Lehman Garrison (https://github.com/lgarrison)\nOriginal author: Nicolas Posner (nicolasposner@gmail.com)\nContributors: Dylan Simon and Alex Chavkin\nRepo: https://github.com/flatironinstitute/fi-slurm-utils";

pub mod cluster_state;
pub mod energy;
pub mod filter;
pub mod jobs;
pub mod list;
pub mod nodes;
pub mod parser;
pub mod partitions;
pub mod site;
pub mod states;
pub mod utils;
