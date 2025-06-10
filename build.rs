// build.rs (Final Linux Node Version)

use std::env;
use std::path::PathBuf;

fn main() {
    // Tell cargo to link against the 'slurm' library.
    // The `module load` command will have already put its location
    // in the linker's standard search path.
    println!("cargo:rustc-link-lib=slurm");

    // Tell cargo to rebuild if the wrapper header changes.
    println!("cargo:rerun-if-changed=wrapper.h");

    // Run bindgen. It will find slurm.h automatically because
    // `module load` has also configured the C include paths.
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file as before.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
