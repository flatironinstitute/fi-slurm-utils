FROM rockylinux/rockylinux:9.8

# Install dependencies for building Slurm and fi-slurm-utils
RUN dnf install -y dnf-plugins-core && \
    dnf config-manager --set-enabled crb && \
    dnf install -y \
        bzip2 \
        clang \
        gcc \
        git \
        munge-devel \
        openssl-devel \
        perl \
        python3.14 \
        python3.14-pip \
        && \
    dnf clean all

# dnf and other system tools are pinned to /usr/bin/python3.9, so expose the
# newer interpreter as `python3`/`pip3` through a venv on PATH instead.
ENV VIRTUAL_ENV=/opt/venv
ENV PATH="${VIRTUAL_ENV}/bin:/root/.cargo/bin:${PATH}"
RUN python3.14 -m venv ${VIRTUAL_ENV} && \
    ${VIRTUAL_ENV}/bin/pip install --no-cache-dir --upgrade pip

ENV RUSTUP_HOME="/root/.rustup"

# Build and install Slurm
ARG SLURM_VERSION=26.05-latest
RUN cd /tmp && \
    curl -O https://download.schedmd.com/slurm/slurm-${SLURM_VERSION}.tar.bz2 && \
    tar -xjf slurm-${SLURM_VERSION}.tar.bz2 && \
    ls -d slurm-*/ | grep -oP '(?<=slurm-).*(?=/)' > /etc/slurm_version && \
    cd slurm-*/ && \
    ./configure --disable-dependency-tracking --disable-static --disable-optimizations && \
    make -C src/api -j$(nproc) version.map && \
    make -C src/api -j$(nproc) libslurm.la && \
    make -C src/api -j$(nproc) install && \
    make -j$(nproc) install-pkgincludeHEADERS && \
    cd / && \
    rm -rf /tmp/*

# Install Rust
ARG RUST_VERSION=1.89
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
    sh -s -- -y --profile minimal --default-toolchain ${RUST_VERSION} -c clippy -c rustfmt && \
    rustup target list --installed -q > /etc/rust_target

WORKDIR /app
