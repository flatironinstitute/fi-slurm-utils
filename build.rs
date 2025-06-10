use std::env;
use std::path::PathBuf;
use std::fs;

fn main() {
    // get the slurm SDK path 
    let slurm_sdk_path = env::var("SLURM_SDK_PATH").expect("SLURM_SDK_PATH environment variable not set. Please set it to the 'output' directory in slurm_docker_rocky");

    let sdk_path = fs::canonicalize(PathBuf::from(slurm_sdk_path))
        .expect("Failed to find SLURM_SDK_PATH. Does the directory exist?");
    
    let lib_dir = sdk_path.join("lib");
    let include_dir = sdk_path.join("include");

    println!("cargo:rustc-link-search=native={}", lib_dir.display());

    // Tell cargo to tell rustc to link the system bzip2
    // shared library.
    println!("cargo:rustc-link-lib=slurm");

    println!("cargo:rerun-if-changed=wrapper.h");

    // configure and run bindgen
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate
        // bindings for.
        .header("wrapper.h")
        // We need to tell bindgen's internal clang where to find the Slurm headers.
        .clang_arg(format!("-I{}", include_dir.display()))
        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
