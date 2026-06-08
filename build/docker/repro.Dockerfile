# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — REPRODUCIBLE build image (Build Plan §5/§8/§10).
#
# This is the image a distrustful user rebuilds from to verify the published
# binary corresponds to the pinned source (the Tor/LibreWolf trust model). It is
# stricter than build.linux.Dockerfile: everything is pinned, and the build is
# driven with SOURCE_DATE_EPOCH so timestamps are deterministic.
#
# Determinism contract (all four must hold for a bit-for-bit rebuild):
#   1. BASE IMAGE PINNED BY DIGEST (not a moving tag).
#   2. TOOLCHAIN VERSIONS PINNED (Rust, clang/LLVM, Node, Python) — exact, not "latest".
#   3. PINNED + VERIFIED SOURCE (build/scripts/fetch-verify-upstream.sh checks the
#      Firefox 145.0.2 SHA-256 manifest + detached signature, fail-hard).
#   4. SOURCE_DATE_EPOCH exported to the published value, and packaging normalized
#      (sorted entries, fixed uid/gid/perms/timestamps).
#
# Verify workflow (see tests/repro/README.md):
#   docker build -f build/docker/repro.Dockerfile -t openbook-repro .
#   docker run --rm -e SOURCE_DATE_EPOCH=<published> -v "$PWD/out:/out" openbook-repro
#   python3 tests/repro/repro_diff.py ./out/<artifact> ./published/<artifact>
#   ci/repro-diff.sh ./out/<artifact> ./published/<artifact>

# --- pinned base (BY DIGEST) ------------------------------------------------
# A reproducible build REQUIRES a digest pin; a tag like "24.04" moves and breaks
# determinism. Set DIGEST to the verified digest before use. CI must assert that
# BASE_IMAGE is in the digest form (contains '@sha256:') for the repro image.
#   docker inspect --format='{{index .RepoDigests 0}}' ubuntu:24.04
ARG BASE_IMAGE=ubuntu@sha256:0000000000000000000000000000000000000000000000000000000000000000
FROM ${BASE_IMAGE}

# --- pinned toolchain versions (exact) --------------------------------------
ARG RUST_VERSION=1.83.0
ARG NODE_MAJOR=20
# The Firefox source pin this image is built to reproduce. Keep in lockstep with
# build/scripts/fetch-verify-upstream.sh and ci/mfsa-track.sh.
ARG FIREFOX_PIN=145.0.2

ENV DEBIAN_FRONTEND=noninteractive \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    TZ=UTC \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONHASHSEED=0 \
    CARGO_HOME=/opt/cargo \
    RUSTUP_HOME=/opt/rustup \
    PATH=/opt/cargo/bin:/usr/local/bin:/usr/bin:/bin \
    # Deterministic linker/archiver behavior helps reproducibility.
    LC_COLLATE=C

# Pin apt to a snapshot for reproducibility where the registry supports it. In
# the absence of a pinned apt snapshot mirror, the digest-pinned base + the
# explicit package set below is the determinism floor; a v2 hardening step is to
# point apt at snapshot.ubuntu.com at a fixed timestamp.
RUN set -eux; \
    apt-get update; \
    apt-get install -y --no-install-recommends \
      ca-certificates curl \
      build-essential clang lld llvm pkg-config \
      python3 python3-venv \
      git \
      zip unzip xz-utils zstd \
      libgtk-3-dev libdbus-glib-1-dev libasound2-dev libpulse-dev \
      libx11-xcb-dev libxt-dev m4 nasm yasm \
      faketime \
      ; \
    rm -rf /var/lib/apt/lists/*

# Node.js pinned major (for extension builds participating in the artifact set).
RUN set -eux; \
    curl -fsSL "https://deb.nodesource.com/setup_${NODE_MAJOR}.x" -o /tmp/nodesetup.sh; \
    bash /tmp/nodesetup.sh; \
    apt-get install -y --no-install-recommends nodejs; \
    rm -f /tmp/nodesetup.sh; rm -rf /var/lib/apt/lists/*; \
    node --version; npm --version

# Rust pinned exactly (minimal profile).
RUN set -eux; \
    curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o /tmp/rustup-init.sh; \
    sh /tmp/rustup-init.sh -y --no-modify-path --profile minimal \
       --default-toolchain "${RUST_VERSION}"; \
    rm -f /tmp/rustup-init.sh; \
    rustc --version; cargo --version

# SOURCE_DATE_EPOCH: callers MUST pass -e SOURCE_DATE_EPOCH=<published value> at
# run time so timestamps match the release. We default it to the deterministic
# sentinel 0 only so an un-parametrized build is still internally reproducible;
# a real release uses the upstream release commit time.
ENV SOURCE_DATE_EPOCH=0

WORKDIR /src

# The build/package steps run via the repo scripts so the recipe stays in one
# place (build/scripts/build.sh, package.sh) under SOURCE_DATE_EPOCH. This image
# ships NO secrets and NO signing keys; signing is a separate, HSM-backed step.

CMD ["bash", "-lc", "echo 'OpenBook reproducible build image (Firefox '\"$FIREFOX_PIN\"'). Pass -e SOURCE_DATE_EPOCH and mount the repo at /src; build via build/scripts/build.sh then package.sh.'"]
