#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

pub mod bindings;
pub mod energy;
pub mod gres;
pub mod nodes;
pub mod jobs;

use std::{collections::HashMap, ffi::CStr};
use chrono::{Date, DateTime, NaiveDateTime, TimeZone, Utc};

use bindings::{acct_gather_energy_t, int_least8_t, node_info_msg_t, slurm_api_version, slurm_free_node_info_msg, slurm_load_node};
use energy::AcctGatherEnergy;
use gres::parse_gres_str;

// This line includes the bindings file that build.rs generates.
//include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

fn main() {
    println!("Calling into the Slurm C library...");

    // FFI calls to C are unsafe. We must wrap them in an unsafe block.
    unsafe {
        // slurm_api_version() is a simple function that returns the API version as an integer.
        // It's a perfect, safe test case.
        let version = slurm_api_version();
        println!("Successfully called slurm_api_version().");
        println!("Slurm API Version: {}", version);
    }
}



