// build.rs

use std::env;
use std::path::PathBuf;

/// Only these get bindings. Without a list, the Slurm headers drag in all of stdio and
/// libc alongside them, which is thousands of items nothing here calls.
const ALLOWED_FUNCTIONS: &[&str] = &["slurm_.*", "slurmdb_.*"];
const ALLOWED_TYPES: &[&str] = &[
    "slurm.*",
    "job_info.*",
    "job_states",
    "node_info.*",
    "node_states",
    "partition_info.*",
    "acct_gather_energy.*",
    "assoc_mgr_info.*",
    "xlist",
    "list_t",
    "list_itr_t",
    // the enums wrapper.h defines to smuggle out SLURM_BIT() macro values
    "bind_.*",
];
const ALLOWED_VARS: &[&str] = &[
    "SLURM_.*",
    "JOB_.*",
    "NODE_.*",
    "PARTITION_.*",
    "SHOW_.*",
    "ASSOC_MGR_.*",
    "INFINITE.*",
    "NO_VAL.*",
];

fn main() {
    // This crate declares the extern functions, so it owns the link directive; the `links`
    // key in Cargo.toml records the same claim for Cargo.
    println!("cargo:rustc-link-lib=slurm");

    // Tell cargo to rebuild if the wrapper header changes.
    println!("cargo:rerun-if-changed=wrapper.h");

    // Run bindgen
    let mut builder = bindgen::Builder::default()
        .header("wrapper.h")
        .wrap_unsafe_ops(true)
        // Slurm spells its state flags SLURM_BIT(n), a function-like macro bindgen cannot
        // expand on its own; the fallback has clang evaluate them so the flag values exist
        .clang_macro_fallback()
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()));

    for pattern in ALLOWED_FUNCTIONS {
        builder = builder.allowlist_function(pattern);
    }
    for pattern in ALLOWED_TYPES {
        builder = builder.allowlist_type(pattern);
    }
    for pattern in ALLOWED_VARS {
        builder = builder.allowlist_var(pattern);
    }

    let bindings = builder.generate().expect("Unable to generate bindings");

    // Get the path to the project's root directory.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
