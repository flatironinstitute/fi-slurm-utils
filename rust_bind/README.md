# Porting to Linux node

In order to rebuild rust bindings in the Rusty node, follow these steps:

1. ssh into a rusty node

2. run the env.sh file to load necessary modules and the local libclang path

3. run `cargo build` as you would for either a release or debug build

In the event of errors, specifically errors highlighting basic C types like `size_t` not being available, double check the contents of `wrapper.h` and `build.rs`. Ensure that both slurm.h and stddef.h are included, and that build.rs is pointing to valid system directories to access the C standard library.
