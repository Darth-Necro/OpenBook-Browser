// SPDX-License-Identifier: MPL-2.0
//
// OpenBook vault host library crate.
//
// This crate implements the Phase 2 cryptographic-lockout native messaging host
// (Build Plan §5). It is exposed as a library (in addition to the
// `openbook-vault-host` binary) so the protocol parser, KDF, and engine can be
// unit- and integration-tested, and so the cargo-fuzz target can link the frame
// parser directly.
//
// Security model summary (read the per-module docs and README for detail):
//   * A 256-bit random Master Key (MK) AEAD-encrypts the profile container.
//   * MK is wrapped under a KEK derived from the user secret (Argon2id) XOR a
//     hardware secret, then HKDF-SHA256. (kdf.rs)
//   * The hardware secret + attempt counter come from a HardwareSecretProvider:
//     real hardware (TPM2 / Secure Enclave, feature-gated skeletons) or a
//     clearly-labeled SOFTWARE fallback with a weaker, advisory guarantee.
//     (hardware.rs)
//   * Cryptographic erasure invalidates the hardware secret FIRST, making the
//     wrapped MK permanently undecryptable. (vault.rs / engine.rs)

pub mod counter;
pub mod engine;
pub mod error;
pub mod hardware;
pub mod kdf;
pub mod protocol;
pub mod vault;

pub use engine::Engine;
pub use error::{ErrorCode, VaultError};
pub use protocol::{parse_frame, Request};
