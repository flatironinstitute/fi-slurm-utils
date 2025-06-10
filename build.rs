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

    let gcc_include_path = "/mnt/sw/nix/store/wsvn32lmr0gf7vwn9y2w5a8b6im0zlf8-gcc-13.3.0/lib/gcc/x86_64-pc-linux-gnu/13.3.0/include";
    let system_include_path = "/usr/local/include";
    let _linux_path = "/usr/include/linux";

    // Run bindgen
    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        //.clang_arg("-I{}", linux_path)
        .clang_arg(format!("-I{}", system_include_path))
        .clang_arg(format!("-I{}", gcc_include_path))
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs directory
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}


