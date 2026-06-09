# SPDX-License-Identifier: MPL-2.0
#
# OpenBook vault-host DISPOSABLE test harness (Build Plan §5.4, §12 Phase 2).
#
# This image exists so the DESTRUCTIVE cryptographic-lockout / erasure tests run
# in a throwaway container against synthetic temp data only. It must NEVER be run
# against real user data or a real Firefox profile, and never bind-mount a real
# profile into the container. The crate's tests already use OS tempdirs; this
# container is the isolation boundary the build plan requires for destructive
# work.
#
# Build & run (from the repo root):
#   docker build -f build/docker/vault-harness.Dockerfile -t openbook-vault-harness .
#   docker run --rm openbook-vault-harness
#
# It builds and tests ONLY the default features (software fallback): no TPM /
# Secure Enclave system libraries are installed, matching the offline-of-hardware
# default build contract.

FROM rust:1.94-slim

# No network access is needed at runtime; the build downloads crates. Keep the
# image minimal. (pkg-config/libssl are not required by the default feature set.)
WORKDIR /opt/openbook

# Copy only what the vault-host crate needs to build and test. We copy the whole
# crate dir; the fuzz/ subdir is a separate crate and is not built here.
COPY native/vault-host/ ./native/vault-host/

# Build then test with default features (software fallback only). Fetching crates
# happens here; the test run is fully offline of any TPM.
RUN cargo build --manifest-path native/vault-host/Cargo.toml

# Default command runs the test suite. `--frozen` is intentionally NOT used so
# crate resolution can complete in environments without a committed lockfile.
CMD ["cargo", "test", "--manifest-path", "native/vault-host/Cargo.toml"]
