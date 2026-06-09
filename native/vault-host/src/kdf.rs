// SPDX-License-Identifier: MPL-2.0
//
// Key-derivation core for the OpenBook vault (Build Plan §5.1).
//
//   KEK = HKDF-SHA256( ikm = Argon2id(secret, salt, params) XOR hardware_secret,
//                      info = "openbook-vault-kek-v1" ) -> 32 bytes
//
// Both the Argon2id output and `hardware_secret` are exactly 32 bytes. We XOR
// them to form the HKDF input keying material (IKM), then HKDF-Extract+Expand to
// the 32-byte Key-Encryption-Key (KEK). The KEK then AEAD-wraps the random
// 256-bit Master Key (see `vault.rs`).
//
// Design rationale:
//   * Argon2id (RFC 9106) makes each offline guess of `secret` expensive. It
//     does NOT save a low-entropy PIN on its own — hence the hardware binding.
//   * XOR-ing in a non-extractable `hardware_secret` means that even with the
//     full ciphertext + metadata, an offline attacker who lacks the device's
//     hardware secret cannot derive the KEK at all. Invalidating that hardware
//     secret is what makes erasure instantaneous and irreversible.
//   * HKDF domain-separates and produces a clean 32-byte key regardless of any
//     structure in the XOR output.

use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use crate::error::{VaultError, Result};

/// HKDF `info` string. Versioned so we can rotate the derivation without
/// ambiguity. Changing this is a breaking change to existing vaults.
pub const KEK_INFO: &[u8] = b"openbook-vault-kek-v1";

/// Length of every key in this module: 256 bits.
pub const KEY_LEN: usize = 32;

/// Argon2id cost parameters, serialized into `vault.json` so a vault created
/// with one parameter set can always be re-derived even if defaults change.
///
/// Defaults (see `default()`): m = 64 MiB, t = 3, p = 1 — a sensible high-cost
/// interactive default per RFC 9106 §4 (the "first recommended option" is
/// m=2 GiB; the "second recommended option" is m=64 MiB, t=3, p=4). We pick
/// m=64 MiB, t=3, p=1 to stay within the memory budget of a desktop native
/// host while keeping per-guess cost high; `p=1` keeps the derivation
/// single-threaded and deterministic across machines (important for the
/// cross-machine determinism the test vectors assert). Tune upward on capable
/// hardware; the value used is always persisted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Argon2Params {
    /// Memory cost in KiB (RFC 9106 `m`).
    pub m_cost_kib: u32,
    /// Iterations / time cost (RFC 9106 `t`).
    pub t_cost: u32,
    /// Degree of parallelism (RFC 9106 `p`).
    pub p_cost: u32,
}

impl Default for Argon2Params {
    fn default() -> Self {
        Argon2Params {
            m_cost_kib: 64 * 1024, // 64 MiB
            t_cost: 3,
            p_cost: 1,
        }
    }
}

impl Argon2Params {
    /// A deliberately cheap parameter set so the test suites run fast.
    ///
    /// SECURITY: NEVER use this for a real vault. It exists so unit AND
    /// integration tests (a separate crate, which cannot see `#[cfg(test)]`
    /// items) can construct fast params. Real vaults use [`Argon2Params::default`]
    /// (64 MiB / t=3 / p=1). This is named loudly to avoid accidental use.
    pub fn testing_cheap() -> Self {
        Argon2Params {
            m_cost_kib: 64, // 64 KiB — far below any safe value
            t_cost: 1,
            p_cost: 1,
        }
    }

    fn to_argon2_params(self) -> Result<Params> {
        Params::new(self.m_cost_kib, self.t_cost, self.p_cost, Some(KEY_LEN))
            .map_err(|e| VaultError::internal(format!("invalid argon2 params: {e}")))
    }
}

/// Run Argon2id over `secret` and `salt`, producing a 32-byte derived key.
/// Returned in a `Zeroizing` buffer so it is wiped on drop.
pub fn argon2id(secret: &[u8], salt: &[u8], params: Argon2Params) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let a2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.to_argon2_params()?);
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    a2.hash_password_into(secret, salt, out.as_mut())
        .map_err(|e| VaultError::internal(format!("argon2id failed: {e}")))?;
    Ok(out)
}

/// XOR two 32-byte buffers into a new buffer (constant time w.r.t. data: a fixed
/// 32-iteration loop with no data-dependent branches).
fn xor32(a: &[u8; KEY_LEN], b: &[u8; KEY_LEN]) -> Zeroizing<[u8; KEY_LEN]> {
    let mut out = Zeroizing::new([0u8; KEY_LEN]);
    for i in 0..KEY_LEN {
        out[i] = a[i] ^ b[i];
    }
    out
}

/// Derive the 32-byte KEK from the user secret and the hardware secret.
///
///   ikm = Argon2id(secret, salt, params) XOR hardware_secret
///   KEK = HKDF-SHA256(ikm, info = KEK_INFO)
///
/// The salt is unique per vault; `hardware_secret` is the 32-byte value from the
/// `HardwareSecretProvider`. Output is zeroized on drop.
pub fn derive_kek(
    secret: &[u8],
    salt: &[u8],
    hardware_secret: &[u8; KEY_LEN],
    params: Argon2Params,
) -> Result<Zeroizing<[u8; KEY_LEN]>> {
    let a2 = argon2id(secret, salt, params)?;
    let ikm = xor32(&a2, hardware_secret);

    // HKDF-Extract (salt = None per the design: the per-vault randomness already
    // lives in the Argon2 salt and the hardware secret) + Expand to 32 bytes.
    let hk = Hkdf::<Sha256>::new(None, ikm.as_ref());
    let mut kek = Zeroizing::new([0u8; KEY_LEN]);
    hk.expand(KEK_INFO, kek.as_mut())
        .map_err(|_| VaultError::internal("hkdf expand failed"))?;
    Ok(kek)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixed inputs for determinism vectors. These are NOT secrets — they are
    // test fixtures. The expected hex values are computed by this very code and
    // pinned so that any future change to the KDF construction is caught.
    const SECRET: &[u8] = b"correct horse battery staple";
    const SALT: &[u8] = b"0123456789abcdef"; // 16-byte fixed salt
    const HW: [u8; KEY_LEN] = [0x11; KEY_LEN];

    #[test]
    fn argon2id_is_deterministic() {
        let p = Argon2Params::testing_cheap();
        let a = argon2id(SECRET, SALT, p).unwrap();
        let b = argon2id(SECRET, SALT, p).unwrap();
        assert_eq!(a.as_ref(), b.as_ref(), "argon2id must be deterministic");
    }

    #[test]
    fn argon2id_known_vector() {
        // Pinned vector for (SECRET, SALT, testing_cheap params). Computed by this
        // implementation; locks the Argon2id config (id variant, v0x13, m=64KiB,
        // t=1, p=1, 32-byte output).
        let p = Argon2Params::testing_cheap();
        let out = argon2id(SECRET, SALT, p).unwrap();
        let got = hex::encode(out.as_ref());
        // Known-answer vector for Argon2id (id variant, v0x13, m=64KiB, t=1, p=1,
        // 32-byte output) over (SECRET, SALT). Computed by this implementation
        // and pinned. If this fails after a dependency bump the Argon2id output
        // changed — investigate (a real algorithm change) before updating it.
        assert_eq!(
            got,
            "6aae361c12fe20f022b47b290a1c3ff26c792515f68c50e44e696c06c9301ffd",
            "argon2id known-answer vector mismatch"
        );
    }

    #[test]
    fn xor_property_holds() {
        let a = [0xaau8; KEY_LEN];
        let b = [0x55u8; KEY_LEN];
        let x = xor32(&a, &b);
        // 0xAA ^ 0x55 == 0xFF
        assert!(x.as_ref().iter().all(|&v| v == 0xff));
        // XOR is its own inverse: (a ^ b) ^ b == a
        let back = xor32(&x, &b);
        assert_eq!(back.as_ref(), &a);
    }

    #[test]
    fn kek_is_deterministic_same_inputs() {
        let p = Argon2Params::testing_cheap();
        let k1 = derive_kek(SECRET, SALT, &HW, p).unwrap();
        let k2 = derive_kek(SECRET, SALT, &HW, p).unwrap();
        assert_eq!(k1.as_ref(), k2.as_ref());
    }

    #[test]
    fn kek_changes_with_hardware_secret() {
        // The whole erasure model rests on this: change/destroy the hardware
        // secret and the KEK is different (i.e. the old wrapped MK is
        // undecryptable).
        let p = Argon2Params::testing_cheap();
        let k1 = derive_kek(SECRET, SALT, &HW, p).unwrap();
        let hw2 = [0x22u8; KEY_LEN];
        let k2 = derive_kek(SECRET, SALT, &hw2, p).unwrap();
        assert_ne!(k1.as_ref(), k2.as_ref());
    }

    #[test]
    fn kek_changes_with_secret() {
        let p = Argon2Params::testing_cheap();
        let k1 = derive_kek(SECRET, SALT, &HW, p).unwrap();
        let k2 = derive_kek(b"a different passphrase entirely", SALT, &HW, p).unwrap();
        assert_ne!(k1.as_ref(), k2.as_ref());
    }

    #[test]
    fn kek_changes_with_salt() {
        let p = Argon2Params::testing_cheap();
        let k1 = derive_kek(SECRET, SALT, &HW, p).unwrap();
        let k2 = derive_kek(SECRET, b"fedcba9876543210", &HW, p).unwrap();
        assert_ne!(k1.as_ref(), k2.as_ref());
    }
}
