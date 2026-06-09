<!-- SPDX-License-Identifier: MPL-2.0 -->
# `openbook-vault-host` — OpenBook cryptographic-lockout native messaging host

Phase 2 flagship data-at-rest feature (Build Plan §5). This Rust binary is the
native messaging host that backs the `vault-ui` extension: it derives keys, binds
them to hardware where available, encrypts the profile container, enforces a
failed-attempt counter, and performs **cryptographic erasure**.

This is the same *class* of mechanism as iOS Data Protection (failed-passcode
erasure) and OS full-disk encryption. The threat it addresses is a **lost or
stolen device / unauthorized physical access** — not a network adversary
(Build Plan §6) and not compelled disclosure (out of scope; see §5.3).

- License: **MPL-2.0**. Rust edition 2021. Binary/crate: `openbook-vault-host`.
- Native host name: `org.openbook.vault_host`. Allowed extension:
  `vault-ui@openbook.browser`.

---

## Native messaging protocol (exact wire contract)

**Transport:** Firefox native messaging framing — a 4-byte message length in
**native byte order** (`u32::from_ne_bytes` / `to_ne_bytes`), followed by that
many bytes of UTF-8 JSON. Max message size is **1 MiB**; larger declared frames
are rejected as `invalid-request` without allocating the claimed size. On EOF the
host exits `0` cleanly. Malformed JSON / bad frames never crash the host — they
yield `invalid-request`.

Every request is a JSON object with at least `{"type": <string>, "id": <number>}`.
Every response echoes `id` and includes `ok: <bool>`.

### Requests and responses

```jsonc
// status
{"type":"status","id":N}
-> {"id":N,"ok":true,
    "state":"uninitialized|locked|unlocked|erased",
    "hardware":"tpm2|secure-enclave|software",
    "maxAttempts":M,"attemptsRemaining":K}

// setup  (maxAttempts default 6)
{"type":"setup","id":N,"secret":<string>,"maxAttempts":<int>,"acknowledgeNoRecovery":true}
-> {"id":N,"ok":true,"state":"locked","hardware":...}
   // errors: already-initialized | weak-secret (software mode) |
   //         no-recovery-not-acknowledged

// unlock
{"type":"unlock","id":N,"secret":<string>}
-> success:       {"id":N,"ok":true,"state":"unlocked"}
-> wrong secret:  {"id":N,"ok":false,"error":"bad-secret","attemptsRemaining":K,"delayMs":D}
-> final failure: {"id":N,"ok":false,"error":"erased","state":"erased"}  // triggers erasure
// Counter increments BEFORE the attempt; resets ONLY on success.

// lock
{"type":"lock","id":N}            -> {"id":N,"ok":true,"state":"locked"}

// erase
{"type":"erase","id":N,"confirm":true}  -> {"id":N,"ok":true,"state":"erased"}
// confirm != true -> invalid-request
```

### Error response shape

```jsonc
{"id":N,"ok":false,"error":"<code>","message":"<human readable>"}  // plus optional fields above
```

Error codes (stable; `vault-ui` matches on these):
`invalid-request, not-initialized, already-initialized, bad-secret, erased,
weak-secret, no-recovery-not-acknowledged, hardware-unavailable, internal`.
Unknown `type` → `invalid-request` (id echoed when present). Malformed JSON / bad
frame → `invalid-request`.

> Note: the wire uses **camelCase** keys (`maxAttempts`, `acknowledgeNoRecovery`,
> `attemptsRemaining`, `delayMs`). The on-disk `vault.json` likewise uses
> camelCase.

---

## Cryptographic design (Build Plan §5.1)

```
Master Key (MK) : 256-bit random (OS CSPRNG). AEAD-encrypts the profile container.
KEK             : Key-Encryption-Key wrapping MK.
KEK = HKDF-SHA256( ikm = Argon2id(secret, salt, params) XOR hardware_secret,
                   info = "openbook-vault-kek-v1" ) -> 32 bytes
hardware_secret : 32 bytes from the HardwareSecretProvider (TPM2 / Secure Enclave
                  / software fallback). Non-extractable on real hardware.
```

- **MK wrap:** XChaCha20-Poly1305 with a random 24-byte nonce; AAD
  `openbook-vault-mk-wrap-v1`. Stored as `wrappedMkHex` in `vault.json`.
- **Container:** XChaCha20-Poly1305 under MK; AAD `openbook-vault-container-v1`;
  stored as `nonce || ciphertext||tag` in `container.enc`.
- **Counter:** monotonic; incremented BEFORE each unlock attempt and persisted
  immediately (power-cycling cannot roll it back); reset to 0 only on success.
- **Cryptographic erasure (final failed attempt OR explicit erase), in order:**
  1. **Invalidate the hardware-sealed secret FIRST** → the KEK becomes
     underivable and the wrapped MK is permanently undecryptable.
  2. Best-effort delete the ciphertext container.
  3. Scrub the wrapped-MK ciphertext from `vault.json`; mark `erased: true`;
     zeroize in-memory key material (`zeroize` crate, `Zeroizing`).
  Order matters: invalidate the key before touching files. File deletion alone is
  **not** the guarantee (SSD wear-leveling/over-provisioning make overwrite
  unreliable; see NIST SP 800-88 "Cryptographic Erase").

### Argon2id parameters (RFC 9106)

Default (persisted per-vault in `vault.json`, so a vault is always re-derivable
even if defaults change later):

| Param | Value | Notes |
|---|---|---|
| variant | Argon2**id** | RFC 9106 recommended variant |
| version | 0x13 (19) | Argon2 v1.3 |
| `m` (memory) | **64 MiB** (65536 KiB) | RFC 9106 §4 "second recommended option" memory |
| `t` (iterations) | **3** | |
| `p` (parallelism) | **1** | single-threaded → deterministic across machines |
| output | 32 bytes | feeds the XOR + HKDF |

Rationale: RFC 9106's first option is m=2 GiB (too heavy for an interactive
desktop unlock); the second is m=64 MiB / t=3 / p=4. We keep m=64 MiB / t=3 but
set **p=1** so derivation is deterministic across machines (the KDF test vectors
assert cross-machine reproducibility) and the host stays single-threaded. Tune
`m`/`t` upward on capable hardware via `Argon2Params`; the value used is always
written to `vault.json`. Argon2id raises per-guess cost but does **not** by
itself save a low-entropy PIN — that is what the hardware binding (and, in
software mode, the forced strong passphrase) is for.

### Escalating-delay schedule

Returned as `delayMs` on consecutive `bad-secret` failures, before the
irreversible erasure, to reduce accidental erasure by a legitimate user
mistyping (Secure-Enclave-style):

| consecutive failures | delayMs |
|---|---|
| 1 | 0 |
| 2 | 1 000 |
| 3 | 5 000 |
| 4 | 30 000 |
| 5 | 60 000 |
| ≥6 | 300 000 |

On hardware backings the TPM / Secure Enclave additionally enforces its own
dictionary-attack lockout independent of this advisory schedule.

---

## Hardware backings & the software-fallback guarantee

`HardwareSecretProvider` abstracts the non-extractable secret + the counter:

- **`SoftwareFallback`** (default build; always compiled). The "hardware" secret
  and counter live in plain files (`hw_secret.bin`, `counter.bin`, mode `0600` on
  Unix) inside the vault directory.
  - **Honest limits:** this defeats a *casual finder* of a running/locked app. It
    does **not** defeat an *offline disk-imaging* adversary, because the secret
    file is on the same image they copy, and the counter can be rolled back by
    restoring the file. The only remaining offline cost is Argon2id over the
    passphrase. Therefore software mode **requires a strong passphrase** (blocks
    all-digit secrets and anything shorter than 12 characters) and `vault-ui`
    must label the attempt limit as **advisory** against disk imaging.
- **`TpmProvider`** (`--features tpm`, OFF by default; needs `libtss2`/`tpm2-tss`).
  Documented **skeleton**: sealed 32-byte secret + NV-index monotonic counter +
  TPM dictionary-attack lockout; `invalidate()` undefines/evicts the secret.
  Currently `probe()` returns `hardware-unavailable` (so even a feature build
  falls back to software until the real integration lands).
- **`SecureEnclaveProvider`** (`--features secure-enclave`, OFF by default; macOS).
  Documented **skeleton**: non-extractable SE key + Keychain-guarded counter;
  `invalidate()` deletes the SE key.

`detect()` picks the best available provider; the default build always returns
`SoftwareFallback`.

---

## Profile-at-rest container (Build Plan §5.2)

This v1 ships a **file-based AEAD container** (`container.enc`): the portable,
cross-platform mechanism (§5.2 recommendation, Option 1 family). `lock` encrypts
the profile blob; `unlock` decrypts it. **Production upgrades** documented for
later phases:

- **Option 1 — userspace encrypted FS (gocryptfs/FUSE):** mount the decrypted
  profile on unlock, unmount on lock. One portable codebase; ships an FS layer.
  **No real FUSE mount is attempted in this crate.**
- **Option 2 — OS-native FDE primitives:** LUKS-on-loopback / fscrypt (Linux),
  encrypted APFS volume / DMG (macOS), BitLocker-backed VHDX (Windows). Audited
  OS crypto; three privileged code paths.

The current host keeps the decrypted blob in memory (zeroized on drop) rather
than mounting it into Gecko's profile path — wiring the container to the live
Firefox profile is the remaining integration (see TODOs).

---

## Build & test

Default build/test use **only** the software fallback and need no TPM/Secure
Enclave system libraries:

```sh
cargo build --manifest-path native/vault-host/Cargo.toml
cargo test  --manifest-path native/vault-host/Cargo.toml
```

Feature builds (require system libraries; not exercised by default CI):

```sh
# TPM 2.0 provider skeleton — needs libtss2 / tpm2-tss dev packages:
cargo build --manifest-path native/vault-host/Cargo.toml --features tpm
# macOS Secure Enclave provider skeleton:
cargo build --manifest-path native/vault-host/Cargo.toml --features secure-enclave
```

### Fuzzing the parser (Build Plan §5.4)

`fuzz/` holds a `cargo-fuzz` harness (targets `parse_frame` and `dispatch`). It
is a separate crate that the normal build/test never touches. It needs nightly +
libFuzzer (not installed in the default image here):

```sh
cargo install cargo-fuzz
cd native/vault-host
cargo +nightly fuzz run parse_frame
cargo +nightly fuzz run dispatch
```

A deterministic, always-on equivalent (no cargo-fuzz needed) lives in
`tests/protocol_robustness.rs`.

---

## Permissions invariant (release blocker — Build Plan §11)

The host binary **and** its native-messaging manifest must be installed
**root-owned and not user-writable** in release packages. A user-writable native
host or manifest is a local privilege-escalation hole (the same class of finding
as user-writable AutoConfig). See `manifests/README.md` for per-OS locations and
exact ownership/mode requirements. Treat any deviation as a release blocker.

---

## DESTRUCTIVE-TESTING GUARDRAIL (Build Plan §5.4, §11, §12 Phase 2)

Lockout, erasure, and container work is **destructive**. Run destructive tests
**only** in disposable VMs/containers against **throwaway** profiles — **never**
a real Firefox profile and **never** the developer host's data.

- All tests in this crate use OS tempdirs (`tempfile`) and synthetic data; they
  never read or write a real profile. The host's default vault directory is its
  own dir (`$OPENBOOK_VAULT_DIR`, else a per-user data dir), **never** a profile.
- The disposable container harness is `build/docker/vault-harness.Dockerfile`
  (and `tests/native/run.sh` from the repo root). Use those for any erasure /
  lockout exercise you would not want to run on your own machine.
- The erase path deliberately invalidates key material; do not point the host at
  data you care about.
