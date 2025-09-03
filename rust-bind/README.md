The rust_bind crate automatically generates and exposes Rust-C bindings against the Slurm job scheduler. 

The `build.rs` script must be pointed to a compiled instance of Slurm and libclang on the local system. If either or both of these systems are not accessible to and visible to rust_bind, it will fail to compile new bindings.

Bindings are intended for consumption by the fi_slurm crate.


# Rebuilding bindings for a new version of Slurm

In order to rebuild rust bindings in the Rusty node, follow these steps:

1. ssh into a rusty node

2. run the env.sh file to load necessary modules and the local libclang path

3. run `cargo build` as you would for either a release or debug build

In the event of errors, specifically errors highlighting basic C types like `size_t` not being available, double check the contents of `wrapper.h` and `build.rs`. Ensure that both slurm.h and stddef.h are included, and that build.rs is pointing to valid system directories to access the C standard library.

A new version of Slurm will result in different bindings. So long as backwards compatibility is maintained, this should not cause the existing fi_slurm code or utilities built on top of it to fail. 
