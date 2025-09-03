# fi-slurm-utils

This repo contains the source code for a set of Rust-based command-line utilities for interacting with the Slurm job scheduler. 

fi-slurm-utils was developed at Flatiron Institute by [Nicolas Posner](https://github.com/nrposner), under the advisement of [Lehman Garrison](https://github.com/lgarrison), [Dylan Simon](https://github.com/dylex), and [Alex Chavkin](https://github.com/alexdotc).

## Utilities

It includes:
- `fi-node`: A CLI and TUI for querying availability of nodes, CPUs, and GPUs.
- `fi-limits`: A CLI for displaying individual and group resource use relative to their assigned resource limits.

These utilities are built on top of custom Rust APIs for the Slurm C library, querying both the Slurm daemon and the Slurm Database daemon, as well as a small API for simple Prometheus queries. These are contained in the fi_slurm, fi_slurm_db, and fi_prometheus sub-crates.

These APIs, in turn, are built on top of a set of Rust-C bindings generated automatically at build time by the rust-bindgen crate.

We hope to build on this to create new utilities, tentatively named `fi-job-top` or `fisq`:
- `fi-job-top`, a TUI interface for resource usage on currently-running jobs
- `fisq`, an `squeue` replacement, perhaps similar to `turm` but with integration with the other tools in our suite and better user ergonomics
