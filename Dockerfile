# Docker file for building the Linux container for Slurm

# Use the specific Rocky Linux version from your cluster as the base image.
FROM rockylinux:8

# Step 1: Install the EPEL (Extra Packages for Enterprise Linux) repository.
# This is a standard repository that provides common HPC software like Slurm
# for Red Hat-based systems. We also update the system.
RUN dnf install -y 'dnf-utils' && \
    dnf install -y epel-release && \
    dnf update -y

# Step 2: Install the core C/C++ development toolchain and the Slurm development package.
# The 'slurm-devel' package is the key. It automatically pulls in other dependencies
# like json-c-devel, hwloc-devel, and the main slurm package itself.
RUN dnf groupinstall -y "Development Tools" && \
    dnf install -y slurm-devel

# Step 3: For convenience, copy the final files we need into a single /output
# folder so they are easy to extract from the container.
# On RHEL-based systems, 64-bit libraries are often in /usr/lib64.
RUN mkdir -p /output/lib && mkdir -p /output/include
RUN cp -r /usr/include/slurm /output/include/
RUN cp /usr/lib64/libslurm.* /output/lib/
