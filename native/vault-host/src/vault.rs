// SPDX-License-Identifier: MPL-2.0
//
// Vault on-disk format + crypto operations (Build Plan §5.1, §5.2).
//
// Files in the vault directory:
//   vault.json        metadata: version, hardware kind, Argon2 params, salt,
//                     wrap nonce, wrapped-MK (ciphertext+tag), max_attempts,
//                     container reference, state. No plaintext secret material.
//   hw_secret.bin     advisory hardware secret (software fallback only).
//   counter.bin       advisory attempt counter (software fallback only).
//   container.enc     the encrypted profile container (this v1 mechanism).
//
// v1 container mechanism (per §5.2 recommendation): a FILE-BASED AEAD container.
// `lock` encrypts a profile blob (a tar of the profile dir, or any bytes) to
// `container.enc`; `unlock` decrypts it back. This is the portable v1 path. The
// README documents the production upgrades: Option 1 (gocryptfs/FUSE userspace
// encrypted FS, mount on unlock / unmount on lock) and Option 2 (OS-native FDE
// primitives: LUKS/fscrypt, APFS encrypted volume, BitLocker VHDX). We do NOT
// attempt real FUSE mounts here.
//
// Key hierarchy:
//   MK  : 256-bit random Master Key. AEAD-encrypts the container.
//   KEK : derived (kdf.rs) from secret + hardware_secret. AEAD-wraps MK.
//
// AEAD: XChaCha20-Poly1305 (24-byte random nonces — large enough that random
// nonces don't realistically collide), via the `chacha20poly1305` crate.

use std::fs;
use std::path::{Path, PathBuf};

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::error::{ErrorCode, Result, VaultError};
use crate::hardware::{HardwareKind, HardwareSecretProvider};
use crate::kdf::{self, Argon2Params, KEY_LEN};

/// Current on-disk metadata version.
pub const VAULT_FORMAT_VERSION: u32 = 1;

/// Metadata file name.
pub const META_FILE: &str = "vault.json";
/// Encrypted container file name (v1 file-based mechanism).
pub const CONTAINER_FILE: &str = "container.enc";

/// AEAD nonce length for XChaCha20-Poly1305.
const XNONCE_LEN: usize = 24;

/// Associated data bound into the MK-wrap AEAD. Ties the ciphertext to this
/// purpose/version so a wrapped MK can't be confused with container ciphertext.
const WRAP_AAD: &[u8] = b"openbook-vault-mk-wrap-v1";
/// Associated data bound into the container AEAD.
const CONTAINER_AAD: &[u8] = b"openbook-vault-container-v1";

/// Persisted vault metadata. Serialized as `vault.json`. Contains NO plaintext
/// key material — only public parameters and the *wrapped* (encrypted) MK.
///
/// Wire/file keys are camelCase (`saltHex`, `wrapNonceHex`, `wrappedMkHex`,
/// `maxAttempts`, `containerRef`) for consistency with the native-messaging
/// protocol's camelCase convention.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultMeta {
    pub version: u32,
    /// Which hardware backing produced the hardware secret (reported on wire).
    pub hardware: HardwareKind,
    /// Argon2id parameters used to derive the KEK (persisted so re-derivation is
    /// always possible even if defaults change).
    pub argon2: Argon2Params,
    /// Per-vault Argon2 salt (random, 16 bytes), hex-encoded.
    pub salt_hex: String,
    /// AEAD nonce used to wrap the MK, hex-encoded (24 bytes).
    pub wrap_nonce_hex: String,
    /// Wrapped MK = AEAD(KEK, MK) ciphertext||tag, hex-encoded.
    pub wrapped_mk_hex: String,
    /// Maximum unlock attempts before cryptographic erasure.
    pub max_attempts: u32,
    /// Reference to the container mechanism/file (v1: a relative file name).
    pub container_ref: String,
    /// Whether the vault has been cryptographically erased. Once true, the
    /// wrapped MK is meaningless (the hardware secret is gone) and we refuse to
    /// transition out of this state.
    pub erased: bool,
}

impl VaultMeta {
    fn salt(&self) -> Result<Vec<u8>> {
        hex::decode(&self.salt_hex)
            .map_err(|_| VaultError::internal("corrupt vault metadata: salt"))
    }
    fn wrap_nonce(&self) -> Result<XNonce> {
        let v = hex::decode(&self.wrap_nonce_hex)
            .map_err(|_| VaultError::internal("corrupt vault metadata: nonce"))?;
        if v.len() != XNONCE_LEN {
            return Err(VaultError::internal("corrupt vault metadata: nonce length"));
        }
        Ok(*XNonce::from_slice(&v))
    }
    fn wrapped_mk(&self) -> Result<Vec<u8>> {
        hex::decode(&self.wrapped_mk_hex)
            .map_err(|_| VaultError::internal("corrupt vault metadata: wrapped mk"))
    }
}

/// The vault: a directory plus its (lazily loaded) metadata. Most operations go
/// through the `engine`, which owns the provider; `Vault` is the crypto/IO core.
pub struct Vault {
    dir: PathBuf,
}

impl Vault {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Vault { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn meta_path(&self) -> PathBuf {
        self.dir.join(META_FILE)
    }
    pub fn container_path(&self) -> PathBuf {
        self.dir.join(CONTAINER_FILE)
    }

    /// True if a metadata file exists (vault has been set up at some point,
    /// possibly erased).
    pub fn exists(&self) -> bool {
        self.meta_path().exists()
    }

    /// Load metadata, or `None` if the vault has never been set up.
    pub fn load_meta(&self) -> Result<Option<VaultMeta>> {
        let path = self.meta_path();
        if !path.exists() {
            return Ok(None);
        }
        let bytes = fs::read(&path)?;
        let meta: VaultMeta = serde_json::from_slice(&bytes)
            .map_err(|_| VaultError::internal("corrupt vault metadata: not valid json"))?;
        if meta.version != VAULT_FORMAT_VERSION {
            return Err(VaultError::internal(format!(
                "unsupported vault format version {}",
                meta.version
            )));
        }
        Ok(Some(meta))
    }

    fn write_meta(&self, meta: &VaultMeta) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let bytes = serde_json::to_vec_pretty(meta)?;
        // Durable write so metadata (incl. the erased flag) survives a crash.
        let path = self.meta_path();
        let tmp = self.dir.join(format!("{META_FILE}.tmp"));
        {
            use std::io::Write as _;
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)?;
            f.write_all(&bytes)?;
            f.flush()?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Create a new vault: generate MK, wrap it under the KEK derived from
    /// `secret` + the provider's hardware secret, write metadata, and create an
    /// (empty) encrypted container. Caller (engine) must have already checked
    /// that no usable vault exists and that policy (weak-secret, no-recovery
    /// ack) is satisfied.
    pub fn create(
        &self,
        secret: &[u8],
        max_attempts: u32,
        params: Argon2Params,
        provider: &dyn HardwareSecretProvider,
    ) -> Result<VaultMeta> {
        fs::create_dir_all(&self.dir)?;

        // 1. Fresh random salt.
        let mut salt = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);

        // 2. Hardware secret (software fallback generates+persists on first use).
        let hw = provider.hardware_secret()?;

        // 3. Derive KEK.
        let kek = kdf::derive_kek(secret, &salt, &hw, params)?;

        // 4. Generate the 256-bit Master Key.
        let mut mk = Zeroizing::new([0u8; KEY_LEN]);
        rand::rngs::OsRng.fill_bytes(mk.as_mut());

        // 5. Wrap MK under KEK with a random nonce.
        let mut wrap_nonce = [0u8; XNONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut wrap_nonce);
        let wrapped = aead_encrypt(kek.as_ref(), &wrap_nonce, mk.as_ref(), WRAP_AAD)?;

        // 6. Initialize the (empty) container so unlock has something to decrypt
        //    and lock has a place to write. Encrypt zero-length plaintext.
        self.write_container(&mk, &[])?;

        // 7. Persist metadata.
        let meta = VaultMeta {
            version: VAULT_FORMAT_VERSION,
            hardware: provider.kind(),
            argon2: params,
            salt_hex: hex::encode(salt),
            wrap_nonce_hex: hex::encode(wrap_nonce),
            wrapped_mk_hex: hex::encode(&wrapped),
            max_attempts,
            container_ref: CONTAINER_FILE.to_string(),
            erased: false,
        };
        self.write_meta(&meta)?;

        // Reset the attempt counter to a clean slate at setup.
        provider.reset_counter()?;

        Ok(meta)
    }

    /// Attempt to unwrap the MK using `secret` + the provider's hardware secret.
    /// Returns the MK on success (zeroized on drop). A wrong secret yields a
    /// `bad-secret` error (the AEAD tag fails). The caller (engine) owns the
    /// counter increment/limit policy around this call.
    pub fn unwrap_mk(
        &self,
        meta: &VaultMeta,
        secret: &[u8],
        provider: &dyn HardwareSecretProvider,
    ) -> Result<Zeroizing<[u8; KEY_LEN]>> {
        let salt = meta.salt()?;
        let hw = provider.hardware_secret()?; // errors -> Erased if secret gone
        let kek = kdf::derive_kek(secret, &salt, &hw, meta.argon2)?;
        let nonce = meta.wrap_nonce()?;
        let wrapped = meta.wrapped_mk()?;
        let pt = aead_decrypt(kek.as_ref(), nonce.as_slice(), &wrapped, WRAP_AAD)
            .map_err(|_| VaultError::new(ErrorCode::BadSecret, "incorrect secret"))?;
        if pt.len() != KEY_LEN {
            return Err(VaultError::internal("unwrapped key has wrong length"));
        }
        let mut mk = Zeroizing::new([0u8; KEY_LEN]);
        mk.copy_from_slice(&pt);
        Ok(mk)
    }

    /// Decrypt the container with `mk`, returning the plaintext profile blob.
    /// Used on unlock (after `unwrap_mk`) to make the profile available.
    pub fn read_container(&self, mk: &Zeroizing<[u8; KEY_LEN]>) -> Result<Zeroizing<Vec<u8>>> {
        let path = self.container_path();
        let raw = fs::read(&path)?;
        if raw.len() < XNONCE_LEN {
            return Err(VaultError::internal("container too short / corrupt"));
        }
        let (nonce, ct) = raw.split_at(XNONCE_LEN);
        let pt = aead_decrypt(mk.as_ref(), nonce, ct, CONTAINER_AAD)
            .map_err(|_| VaultError::internal("container authentication failed"))?;
        Ok(Zeroizing::new(pt))
    }

    /// Encrypt `plaintext` under `mk` and write it as the container (nonce ||
    /// ciphertext||tag). Used on setup (empty) and on lock (the profile blob).
    pub fn write_container(&self, mk: &Zeroizing<[u8; KEY_LEN]>, plaintext: &[u8]) -> Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut nonce = [0u8; XNONCE_LEN];
        rand::rngs::OsRng.fill_bytes(&mut nonce);
        let ct = aead_encrypt(mk.as_ref(), &nonce, plaintext, CONTAINER_AAD)?;
        let mut out = Vec::with_capacity(XNONCE_LEN + ct.len());
        out.extend_from_slice(&nonce);
        out.extend_from_slice(&ct);

        use std::io::Write as _;
        let path = self.container_path();
        let tmp = self.dir.join(format!("{CONTAINER_FILE}.tmp"));
        {
            let mut f = fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&tmp)?;
            f.write_all(&out)?;
            f.flush()?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// Cryptographic erasure (Build Plan §5.1). ORDER MATTERS:
    ///   1. Invalidate the hardware-sealed secret FIRST. After this the KEK is
    ///      underivable and the wrapped MK is permanently undecryptable — the
    ///      data is gone regardless of what happens next.
    ///   2. Best-effort delete the ciphertext container.
    ///   3. Mark metadata erased (and drop the wrapped MK / nonce so even the
    ///      ciphertext-of-the-key is no longer present).
    ///
    /// In-memory key zeroization is handled by `Zeroizing`/drops at the call
    /// sites; this function holds no long-lived key material.
    pub fn erase(&self, provider: &dyn HardwareSecretProvider) -> Result<()> {
        // 1. Invalidate hardware secret FIRST (the decisive, irreversible step).
        provider.invalidate()?;

        // 2. Best-effort delete the container ciphertext.
        let container = self.container_path();
        if container.exists() {
            // Best-effort: failure here does not undo erasure (the key is gone).
            let _ = fs::remove_file(&container);
        }

        // 3. Record erased state in metadata, scrubbing the wrapped MK so the
        //    on-disk file no longer even contains the (now-undecryptable)
        //    ciphertext of the key.
        if let Some(mut meta) = self.load_meta()? {
            meta.erased = true;
            meta.wrapped_mk_hex.clear();
            meta.wrap_nonce_hex.clear();
            self.write_meta(&meta)?;
        } else {
            // No metadata yet (erase before setup): write a minimal erased
            // marker so status reports `erased` deterministically.
            let marker = VaultMeta {
                version: VAULT_FORMAT_VERSION,
                hardware: provider.kind(),
                argon2: Argon2Params::default(),
                salt_hex: String::new(),
                wrap_nonce_hex: String::new(),
                wrapped_mk_hex: String::new(),
                max_attempts: 0,
                container_ref: CONTAINER_FILE.to_string(),
                erased: true,
            };
            self.write_meta(&marker)?;
        }
        Ok(())
    }
}

/// AEAD-encrypt with XChaCha20-Poly1305. Returns ciphertext||tag. `key` must be
/// 32 bytes; we accept a slice so callers can pass `Zeroizing<[u8;32]>` without
/// fighting deref/array-ref coercions.
fn aead_encrypt(key: &[u8], nonce: &[u8], pt: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| VaultError::internal("aead key wrong length"))?;
    if nonce.len() != XNONCE_LEN {
        return Err(VaultError::internal("aead nonce wrong length"));
    }
    let nonce = XNonce::from_slice(nonce);
    cipher
        .encrypt(nonce, Payload { msg: pt, aad })
        .map_err(|_| VaultError::internal("aead encrypt failed"))
}

/// AEAD-decrypt with XChaCha20-Poly1305. `ct` is ciphertext||tag. A tag mismatch
/// (wrong key / tampering) returns an error the caller maps appropriately.
fn aead_decrypt(key: &[u8], nonce: &[u8], ct: &[u8], aad: &[u8]) -> Result<Vec<u8>> {
    if nonce.len() != XNONCE_LEN {
        return Err(VaultError::internal("aead nonce wrong length"));
    }
    let cipher = XChaCha20Poly1305::new_from_slice(key)
        .map_err(|_| VaultError::internal("aead key wrong length"))?;
    let nonce = XNonce::from_slice(nonce);
    cipher
        .decrypt(nonce, Payload { msg: ct, aad })
        .map_err(|_| VaultError::internal("aead decrypt failed"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::SoftwareFallback;
    use tempfile::tempdir;

    fn provider(dir: &Path) -> SoftwareFallback {
        SoftwareFallback::new(dir.to_path_buf())
    }

    #[test]
    fn create_then_unwrap_roundtrip() {
        let d = tempdir().unwrap();
        let v = Vault::new(d.path());
        let p = provider(d.path());
        let params = Argon2Params::testing_cheap();
        let secret = b"a strong enough passphrase";

        let meta = v.create(secret, 6, params, &p).unwrap();
        assert!(!meta.erased);
        assert_eq!(meta.hardware, HardwareKind::Software);

        let loaded = v.load_meta().unwrap().unwrap();
        let mk = v.unwrap_mk(&loaded, secret, &p).unwrap();

        // Container roundtrip: write a profile blob, read it back.
        let blob = b"profile bytes: cookies, history, tokens";
        v.write_container(&mk, blob).unwrap();
        let got = v.read_container(&mk).unwrap();
        assert_eq!(got.as_slice(), blob);
    }

    #[test]
    fn wrong_secret_fails_with_bad_secret() {
        let d = tempdir().unwrap();
        let v = Vault::new(d.path());
        let p = provider(d.path());
        let params = Argon2Params::testing_cheap();
        v.create(b"the right passphrase here", 6, params, &p).unwrap();

        let meta = v.load_meta().unwrap().unwrap();
        let err = v.unwrap_mk(&meta, b"the WRONG passphrase here", &p).unwrap_err();
        assert_eq!(err.code, ErrorCode::BadSecret);
    }

    #[test]
    fn erase_makes_mk_permanently_unrecoverable() {
        let d = tempdir().unwrap();
        let v = Vault::new(d.path());
        let p = provider(d.path());
        let params = Argon2Params::testing_cheap();
        let secret = b"correct passphrase value";
        v.create(secret, 6, params, &p).unwrap();

        // Sanity: unwrap works before erase.
        let meta = v.load_meta().unwrap().unwrap();
        assert!(v.unwrap_mk(&meta, secret, &p).is_ok());

        // Erase, then prove the MK can no longer be recovered EVEN with the
        // correct secret, because the hardware secret was invalidated first.
        v.erase(&p).unwrap();
        let meta_after = v.load_meta().unwrap().unwrap();
        assert!(meta_after.erased);
        // Wrapped MK ciphertext is scrubbed from metadata.
        assert!(meta_after.wrapped_mk_hex.is_empty());
        // And even if we hadn't scrubbed it, deriving the KEK now uses a
        // regenerated (different) hardware secret, so unwrap is impossible.
        let err = v.unwrap_mk(&meta, secret, &p);
        assert!(err.is_err(), "MK must be unrecoverable after erase");

        // Container ciphertext is gone too.
        assert!(!v.container_path().exists());
    }

    #[test]
    fn tampered_container_fails_authentication() {
        let d = tempdir().unwrap();
        let v = Vault::new(d.path());
        let p = provider(d.path());
        let params = Argon2Params::testing_cheap();
        let secret = b"correct passphrase value";
        v.create(secret, 6, params, &p).unwrap();
        let meta = v.load_meta().unwrap().unwrap();
        let mk = v.unwrap_mk(&meta, secret, &p).unwrap();
        v.write_container(&mk, b"sensitive").unwrap();

        // Flip a byte in the container ciphertext.
        let path = v.container_path();
        let mut bytes = std::fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 0xff;
        std::fs::write(&path, &bytes).unwrap();

        let err = v.read_container(&mk).unwrap_err();
        assert_eq!(err.code, ErrorCode::Internal); // auth failure surfaces as internal
    }
}
