This repository will hold the source code for a series of Rust-based utilities developed at the Flatiron Institute by Lehman Garrison, Nicolas Posner, Dylan Simon, and Alex Chavkin. We will begin this project by aiming at creating a node-visibility utility tentatively named `fi-node`, which duplicates and extends the functionality of `carriero/bin/featureInfo`. 

This is a user-facing CLI tool fitting the following parameters:

- Should provide visibility into availability of nodes by type (icelake, rome, genoa, a100, v100, h100, etc)
- Provide easy readability and context to the user, with 
	- Columns and unit headers
	- Aligned columns
	- Showing available rather than unavailable resources by default
	- Hiding GPUs by default (if you're running a GPU job, you'll know it)
 - Stretch: expand CLI capacity to a TUI, with simple graphical representations of historical resource availability

As possible, we can expand on this framework with other utilities, such as the tentatively named `fi-job-top` or `fisq`:
- `fi-job-top`, a TUI interface for resource usage on currently-running jobs
- `fisq`, an `squeue` replacement, perhaps similar to `turm` but with integration with the other tools in our suite and better user ergonomics


## Technical considerations

We broadly have two approaches for constructing these tools: on the one hand, we can rely on the existing Slurm CLI tooling, such as `sinfo`, which reduces some of our power and flexibility and introduces more overhead for frequent system calls, but should be more resilient to breaking or undocumented changes in the underlying software. 

On the other hand, we could interface with libslurm directly, binding using `rust-bindgen` to access the C representations from Rust directly. This is more powerful, but requires up-front work creating and verifying the integrity of the bindings, and leaves us at the mercy of subtle breaking changes in the underlying Slurm types or functions. 

For the time being, we will pursue a libslurm implementation. We will default to `clap` for CLI production and `ratatui` for TUI production.


## Tasks
- [ ] Identify capabilities of the `carriero` featureInfo utility and map out its dependencies in libslurm (via PySlurm)
- [ ] Produce a `rust-bindgen` binding of necessary Slurm types
- [ ] Test the integrity of the `fi-node` bindings in a sequestered node
- [ ] Construct the 'business logic' in duplication of `carriero`
- [ ] Obtain user feedback on the benefits and pain points of the new utilities and determine how best to integrate them into existing workflows
- [ ] (Stretch) Develop a TUI representation of `fi-node`

