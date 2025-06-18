The fi_slurm crate contains a set of common APIs built atop automatically-generated Rust-C bindings for Slurm 24.11.5.

It is designed as a common base and reference for utilities providing visibility into the Slurm scheduler.

Future versions of Slurm offer opportunities to improve fi_slurm. Updating to Slurm 25 and having direct access to by-node CPU allocation data would allow us to deprecate the node_to_job map construction, simplifying and speeding up initialization.

