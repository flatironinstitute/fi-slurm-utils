# fi-slurm-utils

This repo contains the source code for a set of Rust-based command-line utilities for interacting with the Slurm job scheduler. 

fi-slurm-utils was developed at Flatiron Institute by [Nicolas Posner](https://github.com/nrposner), under the advisement of [Lehman Garrison](https://github.com/lgarrison), [Dylan Simon](https://github.com/dylex), and [Alex Chavkin](https://github.com/alexdotc).

## Utilities

It includes:
- `fi-nodes`: A CLI and TUI for querying availability of nodes, CPUs, and GPUs.
- `fi-limits`: A CLI for displaying individual and group resource use relative to their assigned resource limits.

These utilities are built on top of custom Rust APIs for the Slurm C library, querying both the Slurm daemon and the Slurm Database daemon, as well as a small API for simple Prometheus queries. These are contained in the fi_slurm, fi_slurm_db, and fi_prometheus sub-crates.

These APIs, in turn, are built on top of a set of Rust-C bindings generated automatically at build time by the rust-bindgen crate.

We hope to build on this to create new utilities, tentatively named `fi-job-top` or `fisq`:
- `fi-job-top`, a TUI interface for resource usage on currently-running jobs
- `fisq`, an `squeue` replacement, perhaps similar to `turm` but with integration with the other tools in our suite and better user ergonomics

## Building
The main build complexity of fi-slurm-utils is in the fi-slurm-bind package, because it runs rust bindgen. bindgen requires libclang and the Slurm development headers. If libclang is not found, you may need to set `LIBCLANG_PATH`. The Slurm development headers are usually installed as part of the `slurm-devel` package. The fi_surm_bind crate links against `libslurm`, so this library is required at runtime.

With these dependencies installed, the build procedure is simply:
```console
cargo build --release
```

## License
Copyright 2025 The Simons Foundation, Inc.

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

http://www.apache.org/licenses/LICENSE-2.0

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.

SPDX-License-Identifier: Apache-2.0
