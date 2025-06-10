
// This line includes the bindings file that build.rs generates.
include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

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
