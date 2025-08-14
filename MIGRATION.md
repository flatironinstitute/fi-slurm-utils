If you're reading this, you are a user with admin permissions on a Slurm cluster who wants to employ these tools on your own cluster. This document provides high-level guidance for doing so. 

Fi-node is not a commercial product, and is provided as-is with no obligation or warranty. It is, first and foremost, a platform for the development and deployment of cluster diagnostic tools. For further guidance and support, contact nicolasposner@gmail.com.


The original version of fi-node was developed and deployed on a Rocky Linux release 8.10 (Green Obsidian) cluster with the following packages: `hwloc/2.11.1` `rust/1.85.0` `gcc/13.3` `llvm/19.1.7`.

fi-node requires access to the locally running instance of slurm, `libslurm.so`. It must bind directly against this, and not any other, previus version of Slurm that may remain in the cluster.

To update fi-node for a new version of Slurm or Rust, it is necessary only to run `cargo build --release` from the top-level directory. It will go through the process of binding, checking, and rebuilding all utilities. In the event that this build fails due to breaking backwards-compatibility, the error messages from this failure will guide the necessary steps to fix the utilities. For further support, contact nicolasposner@gmail.com.
