The fi_slurm_bind crate automatically generates and exposes Rust-C bindings against the Slurm job scheduler. 

The `build.rs` script must be pointed to a compiled instance of Slurm and libclang on the local system. If either or both of these systems are not accessible to and visible to fi_slurm_bind, it will fail to compile new bindings.

Bindings are intended for consumption by the fi_slurm crate.
