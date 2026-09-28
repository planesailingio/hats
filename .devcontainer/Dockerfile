# Dev container to smoke-test hats on Linux.
#
# Tests the portable core: hats builds, scaffolds the base skeletons and
# per-hat files, and switches profiles with the right identity and an
# isolated kubeconfig. No Homebrew: hats needs nothing but git and a shell.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Base packages: build deps for Homebrew and Rust, zsh, git, curl, locales, sudo.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential procps curl file git ca-certificates \
      zsh sudo locales tzdata less pkg-config libssl-dev \
    && locale-gen en_US.UTF-8 \
    && rm -rf /var/lib/apt/lists/*

ENV LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8

# Non-root user (Homebrew refuses to run as root). Passwordless sudo for setup.
ARG USER=dev
RUN useradd -m -s /usr/bin/zsh ${USER} \
    && echo "${USER} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/${USER} \
    && chmod 0440 /etc/sudoers.d/${USER}
USER ${USER}
WORKDIR /home/${USER}

# Rust, to build hats from the mounted source.
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable
ENV PATH="/home/${USER}/.cargo/bin:${PATH}"

# The repo is mounted at /workspace by devcontainer.json; the harness builds and
# applies from there.
CMD ["zsh"]
