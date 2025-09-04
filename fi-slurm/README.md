The fi_slurm crate contains a set of common APIs built atop automatically-generated Rust-C bindings for Slurm 24.11.5.

It is designed as a common base and reference for utilities providing visibility into the Slurm scheduler.

Future versions of Slurm offer opportunities to improve fi_slurm. Updating to Slurm 25 and having direct access to by-node CPU allocation data would allow us to deprecate the node_to_job map construction, simplifying and speeding up initialization.

For details on the structure of the fi_slurm API, refer to the API.md file in the same directory.

This API was developed by Nicolas Posner and Lehman Garrison at the Scientific Computing Core of the Flatiron Institute. To implement this in your own cluster, refer to the MIGRATION.md file in the parent directory. For support and consultation, contact nicolasposner@gmail.com or file an issue in the https://github.com/flatironinstitute/fi-slurm-utils repo.
