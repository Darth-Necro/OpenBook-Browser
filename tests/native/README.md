<!-- SPDX-License-Identifier: MPL-2.0 -->
# `tests/native/` — native-host test entry point

The OpenBook native messaging host (vault) is a Rust crate at
[`native/vault-host`](../../native/vault-host). Its tests live **with the crate**
(idiomatic Rust): unit tests inline in each module (`#[cfg(test)]`) and
integration tests under `native/vault-host/tests/`.

This directory provides a thin top-level runner so the native-host suite can be
invoked the same way as the repo's other test groups.

## Run

```sh
tests/native/run.sh
```

That runs `cargo test --manifest-path native/vault-host/Cargo.toml` with
**default features** (the software fallback — no TPM/Secure Enclave system
libraries needed).

## What is covered

- **Unit tests** (in-crate): protocol framing + parsing, KDF determinism /
  known-answer vectors / XOR property, hardware provider (secret stability,
  counter persistence, invalidation), counter policy + escalating delays, vault
  create/unlock/erase crypto.
- **Integration** (`native/vault-host/tests/vault_integration.rs`): full
  setup → unlock → lock → unlock → status round-trip; wrong-secret increment +
  escalating delay; counter survives a simulated process restart; reaching
  `maxAttempts` triggers cryptographic erasure and the vault becomes permanently
  undecryptable; explicit erase; weak-secret rejection in software mode.
- **Protocol robustness** (`native/vault-host/tests/protocol_robustness.rs`):
  deterministic "fuzz the parser" — many malformed frames + structured mutations;
  asserts no panic and `invalid-request`.
- **Fuzzing** (optional, CI): `native/vault-host/fuzz/` cargo-fuzz targets
  (`parse_frame`, `dispatch`). Need nightly + libFuzzer; not run by `run.sh`.

## Hardware-backed backends

`--features tpm` requires `libtss2` / `tpm2-tss` on the host; `--features
secure-enclave` requires macOS. These are **not** run by `run.sh` and need real
hardware to exercise meaningfully (do so in a disposable VM only).

## Destructive-testing guardrail

The vault tests exercise erasure/lockout, but only against OS temp dirs and
synthetic data — never a real Firefox profile, never the dev host's data
(Build Plan §5.4, §12 Phase 2). For hand-run destructive experiments use the
disposable container: `build/docker/vault-harness.Dockerfile`.
