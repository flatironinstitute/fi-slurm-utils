# fi-slurm-utils

This repo contains the source code for a set of Rust-based command-line utilities for interacting with the Slurm job scheduler. 

fi-slurm-utils was developed at Flatiron Institute by [Nicolas Posner](https://github.com/nrposner), under the advisement of [Lehman Garrison](https://github.com/lgarrison), [Dylan Simon](https://github.com/dylex), and [Alex Chavkin](https://github.com/alexdotc).

<!-- TODO: screenshot of tree view here -->

## Overview

The primary utilities are:
- `fi-nodes`: a CLI and TUI for querying availability of nodes, CPUs, and GPUs.
- `fi-limits`: a CLI for displaying individual and group resource use relative to their assigned resource limits.

These utilities are built on top of a set of Rust interfaces to Slurm's C APIs:
- `fi-slurm`: a high-level Rust API (consisting of owning Rust types) to the `slurm.h` API.
- `fi-slurm-db`: likewise, but for the `slurmdb.h` API.

These APIs are built on a rust-bindgen package:
- `fi-slurm-bind`: a low-level, automatically-generated interface to the `slurm.h` and `slurmdb.h` APIs

The `fi-nodes` utility also has an optional interactive TUI (terminal user interface) that queries Prometheus:
- `fi-prometheus`: an API to query Prometheus for time-series data about the Slurm cluster
The TUI is under development, and its compilation can be enabled with the `"tui"` Cargo feature.

We hope to use this foundation to build more utilities, including:
- `fi-job-top`, a TUI interface for viewing node resource usage on currently-running jobs

## Building
The main build complexity of fi-slurm-utils is in the fi-slurm-bind package, because it runs rust bindgen. bindgen requires libclang and the Slurm development headers. If libclang is not found, you may need to set `LIBCLANG_PATH`. The Slurm development headers are usually installed as part of the `slurm-devel` package. The fi_surm_bind crate links against `libslurm`, so this library is required at runtime.

With these dependencies installed, the build procedure is simply:
```console
cargo build --release
```

To enable the `fi-nodes` TUI (brings in Prometheus dependencies):
```console
cargo build --release --features tui
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
