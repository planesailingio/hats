# Ubuntu image with hats installed and ready to use.
#
#   docker build -t hats .
#   docker run --rm -it hats
#
# install.sh runs at build time with CI=1, so Homebrew and hats are baked into
# the image and the container starts usable. Nothing is configured: `hats init`
# is interactive and belongs to whoever runs the container, not to the build.
#
# This is not .devcontainer/Dockerfile. That one builds hats from the mounted
# source to test it on Linux; this one installs the released formula from the
# tap, which is what a user gets.
FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive

# Homebrew's own prerequisites, plus zsh because that is the shell hats targets.
RUN apt-get update && apt-get install -y --no-install-recommends \
      build-essential procps curl file git ca-certificates \
      zsh sudo locales tzdata less \
    && locale-gen en_US.UTF-8 \
    && rm -rf /var/lib/apt/lists/*

ENV LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8

# Homebrew refuses to run as root, so the image needs an ordinary user. sudo is
# passwordless because the Homebrew installer asks for it during setup.
ARG USER=dev
RUN useradd -m -s /usr/bin/zsh ${USER} \
    && echo "${USER} ALL=(ALL) NOPASSWD:ALL" > /etc/sudoers.d/${USER} \
    && chmod 0440 /etc/sudoers.d/${USER}

USER ${USER}
WORKDIR /home/${USER}

# Copied rather than curled so the image builds from this checkout, and a change
# to the script invalidates the layer below it.
COPY --chown=${USER}:${USER} install.sh /tmp/install.sh

# CI=1 answers the Homebrew prompt. Without it the build would hang waiting for
# a terminal that a docker build does not have.
RUN CI=1 sh /tmp/install.sh && rm /tmp/install.sh

# brew shellenv is what install.sh evaluated for itself; bake the same PATH in
# so every later layer, and every shell in the container, finds brew and hats.
ENV PATH="/home/linuxbrew/.linuxbrew/bin:/home/linuxbrew/.linuxbrew/sbin:/home/${USER}/.local/bin:${PATH}"

# Interactive shells get brew's environment properly (MANPATH, INFOPATH too).
RUN echo 'eval "$(/home/linuxbrew/.linuxbrew/bin/brew shellenv)"' >> /home/${USER}/.zshrc

RUN hats --version && hats doctor || true

CMD ["zsh"]
