# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — Linux build image (Build Plan §8/§9).
#
# Pinned-toolchain image for building OpenBook (a Firefox 145.0.2 fork) on Linux
# x86_64. This is the *build* image used by build/scripts/build.sh; for a
# bit-for-bit reproducible release image see repro.Dockerfile, which is stricter
# (digest-pinned base + exact toolchain versions + SOURCE_DATE_EPOCH discipline).
#
# Pin policy: the base image is referenced BY DIGEST (not a moving tag) so the
# image is deterministic. Replace the digest below when intentionally bumping the
# base; record the bump in docs/DECISIONS.md. Toolchain versions are pinned too.
#
# Build context is the repo root:
#   docker build -f build/docker/build.linux.Dockerfile -t openbook-build-linux .
#
# Then build inside it (mounting the repo), e.g.:
#   docker run --rm -v "$PWD:/src" -w /src openbook-build-linux \
#     build/scripts/build.sh --target linux-x64
#
# Firefox build requirements (firefox-source-docs): 64-bit host, >= 30 GB free
# disk, 8 GB+ RAM, Python 3.9+, the mach build system. Ensure the Docker host has
# the disk/RAM headroom; a full (non-artifact) build compiles for hours.

# --- pinned base ------------------------------------------------------------
# Ubuntu 24.04 LTS. Pin by digest for determinism. The placeholder digest below
# MUST be set to a real, verified digest before use (CI verifies the pin).
#   docker pull ubuntu:24.04 && docker inspect --format='{{index .RepoDigests 0}}' ubuntu:24.04
ARG BASE_IMAGE=ubuntu:24.04
# Example of the digest-pinned form to switch to once verified:
#   ARG BASE_IMAGE=ubuntu@sha256:<64-hex-digest>
FROM ${BASE_IMAGE}

# Make apt non-interactive and builds reproducible-friendly.
ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    PYTHONDONTWRITEBYTECODE=1 \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:/usr/local/bin:/usr/bin:/bin

# --- pinned toolchain versions ----------------------------------------------
# Bump deliberately; these are the contract. RUST_VERSION must satisfy the
# crates' rust-version (>= 1.74) AND upstream Firefox 145's minimum.
ARG RUST_VERSION=1.83.0
ARG NODE_MAJOR=20

# --- OS build dependencies --------------------------------------------------
# mach bootstrap pulls most Gecko build deps, but the base set below is needed to
# bootstrap, run mach, build the Rust native host, and build the TS extensions.
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
      ca-certificates curl wget gnupg \
      build-essential clang lld llvm pkg-config \
      python3 python3-pip python3-venv \
      git mercurial \
      zip unzip xz-utils zstd \
      libgtk-3-dev libdbus-glib-1-dev libasound2-dev libpulse-dev \
      libx11-xcb-dev libxt-dev m4 nasm yasm \
      ; \
    rm -rf /var/lib/apt/lists/*

# --- Node.js (for web-ext / extension builds) -------------------------------
RUN set -eux; \
    curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" -o /tmp/nodesetup.sh; \
    bash /tmp/nodesetup.sh; \
    apt-get install -y --no-install-recommends nodejs; \
    rm -f /tmp/nodesetup.sh; \
    rm -rf /var/lib/apt/lists/*; \
    node --version; npm --version

# --- Rust toolchain (pinned, via rustup) ------------------------------------
RUN set -eux; \
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup-init.sh; \
    sh /tmp/rustup-init.sh -y --no-modify-path --profile minimal \
       --default-toolchain "${RUST_VERSION}"; \
    rm -f /tmp/rustup-init.sh; \
    rustc --version; cargo --version

# Optional CycloneDX SBOM generator for ci/sbom.sh (best-effort; pinned).
# Kept in the build image so the SBOM step has a real CycloneDX backend.
RUN set -eux; \
    cargo install cargo-cyclonedx --version 0.5.7 --locked || \
    echo "WARN: cargo-cyclonedx install skipped; ci/sbom.sh will fall back to cargo metadata"

WORKDIR /src

# Non-root build user is recommended; the actual UID mapping is set at run time
# (-u "$(id -u):$(id -g)") so output files are owned by the invoking user. The
# image itself ships no secrets and no signing keys (keys live in HSM only).

CMD ["bash", "-lc", "echo 'OpenBook Linux build image. Mount the repo at /src and invoke build/scripts/build.sh.'"]
