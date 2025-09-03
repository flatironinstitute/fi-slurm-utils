// build.rs (Final Linux Node Version)

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to link against the 'slurm' library.
    // The `module load` command will have already put its location
    // in the linker's standard search path.

    // Tell cargo to rebuild if the wrapper header changes.
    println!("cargo:rerun-if-changed=wrapper.h");

    // Run bindgen
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    // Get the path to the project's root directory.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
