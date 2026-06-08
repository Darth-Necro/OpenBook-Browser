#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# Run the OpenBook native-host (vault) test suite from the repo root.
#
# This runs the Rust unit + integration + protocol-robustness tests for
# native/vault-host using only the DEFAULT features (software fallback) — no TPM
# or Secure Enclave system libraries required.
#
# DESTRUCTIVE-TESTING NOTE (Build Plan §5.4): the vault tests exercise
# cryptographic erasure and lockout, but they do so ONLY against OS temp dirs and
# synthetic data — never a real Firefox profile. For any hand-run destructive
# experiment, prefer the disposable container in
# build/docker/vault-harness.Dockerfile.
set -euo pipefail

# Resolve the repo root from this script's location so it works from anywhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
MANIFEST="${REPO_ROOT}/native/vault-host/Cargo.toml"

echo "[openbook] Running vault-host tests (default features: software fallback)"
echo "[openbook] Manifest: ${MANIFEST}"

cargo test --manifest-path "${MANIFEST}"

echo
echo "[openbook] Default-feature tests passed."
echo "[openbook] NOTE: the hardware backends are feature-gated and need system libs:"
echo "[openbook]   --features tpm             requires libtss2 / tpm2-tss on the host"
echo "[openbook]   --features secure-enclave  requires macOS (Security framework)"
echo "[openbook] Those features are intentionally NOT run here."
echo "[openbook] Fuzzing (optional): cd native/vault-host && cargo +nightly fuzz run parse_frame"
