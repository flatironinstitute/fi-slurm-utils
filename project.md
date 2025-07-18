Within this project, we are creating a set of Rust utilities on top of the Slurm HPC cluster job scheduling software. The workspace is organized as follows:

`rust_bind`: Automatically generate Rust bindings for Slurm from the local libslurm.so file.
`fi_slurm.`: Create a safe Rust API for interacting with select Slurm types.
`fi_prometheus`: Provide a Rust API for interfacing with the Prometheus database.
`fi-node`: Create applications using these APIs, including several types of reports about the current state of the cluster, and a terminal user interface (TUI) displaying historical data gathered from Prometheus.
`fi-job-top`: A future project for displaying job resource usage statistics in a TUI.
`carriero`: A reference Python implementation of Slurm reporting, no longer relevant to current development.
