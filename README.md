This workspace contains the source code for a series of Rust-based command-line utilities developed at the Flatiron Institute by **Lehman Garrison**, **Nicolas Posner**, **Dylan Simon**, and **Alex Chavkin**. 

It includes:
`fi-node`: A set of utilities for getting live diagnostics from a running Slurm cluster about node availability and usage.
`fi-limits`: A set of utilities for getting information about individual and group resource use relative to their assigned resource limits.

These utilities are built on top of custom Rust APIs for the Slurm C library, querying both the Slurm daemon and the Slurm Database daemon, as well as a small API for simple Prometheus queries. These are contained in the fi_slurm, fi_slurm_db, and fi_prometheus sub-crates.

These APIs, in turn, are built on top of a set of Rust-C bindings generated automatically at build time by the rust-bindgen crate.

We hope to build on this to create new utilities, tentatively named `fi-job-top` or `fisq`:
- `fi-job-top`, a TUI interface for resource usage on currently-running jobs
- `fisq`, an `squeue` replacement, perhaps similar to `turm` but with integration with the other tools in our suite and better user ergonomics

