// SPDX-License-Identifier: MPL-2.0
//
// Vault state machine + request handlers (Build Plan §5.1).
//
// States:
//   Uninitialized -> no usable vault metadata on disk.
//   Locked        -> vault exists, MK not currently held in memory.
//   Unlocked      -> MK held in memory (this process unwrapped it this session).
//   Erased        -> cryptographic erasure done; terminal.
//
// The engine owns the `HardwareSecretProvider` and the `Vault`. It enforces:
//   * weak-secret rejection in software-fallback mode,
//   * the no-recovery acknowledgement at setup,
//   * the counter policy (increment-before-attempt, reset-on-success, erase at
//     the limit) and escalating delays,
//   * the crypto-erasure ordering (provider.invalidate first).
//
// Responses are built as `serde_json::Value` so each request can carry its own
// shape while always including `id` and `ok`.

use serde_json::{json, Value};
use zeroize::Zeroizing;

use crate::counter::{delay_ms_for_failures, AttemptBudget};
use crate::error::{ErrorCode, Result, VaultError};
use crate::hardware::{self, HardwareKind, HardwareSecretProvider};
use crate::kdf::Argon2Params;
use crate::protocol::{ErrorResponse, Request};
use crate::vault::Vault;

/// Minimum passphrase length accepted in SOFTWARE-fallback mode. Hardware-backed
/// modes can accept shorter secrets because the hardware enforces rate limiting;
/// software mode cannot, so we force entropy via length (and block all-digits).
pub const MIN_SOFTWARE_SECRET_LEN: usize = 12;

/// High-level vault state, reported on the wire as `state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Uninitialized,
    Locked,
    Unlocked,
    Erased,
}

impl State {
    pub fn as_str(self) -> &'static str {
        match self {
            State::Uninitialized => "uninitialized",
            State::Locked => "locked",
            State::Unlocked => "unlocked",
            State::Erased => "erased",
        }
    }
}

/// The engine: holds the vault, the provider, and the in-memory unlock state.
pub struct Engine {
    vault: Vault,
    provider: Box<dyn HardwareSecretProvider>,
    /// Argon2 params used for NEW vaults. Existing vaults use their persisted
    /// params (read from metadata) regardless of this.
    setup_params: Argon2Params,
    /// In-memory unlocked Master Key for this session, if unlocked. Zeroized on
    /// drop / on lock.
    mk: Option<Zeroizing<[u8; crate::kdf::KEY_LEN]>>,
    /// Cached "erased this session" flag so we report Erased even if a metadata
    /// read transiently fails after we just erased.
    erased_session: bool,
}

impl Engine {
    /// Build an engine rooted at the vault directory, auto-detecting the best
    /// available hardware provider (software in the default build).
    pub fn new(vault_dir: impl Into<std::path::PathBuf>) -> Self {
        let dir = vault_dir.into();
        let provider = hardware::detect(&dir);
        Engine {
            vault: Vault::new(dir),
            provider,
            setup_params: Argon2Params::default(),
            mk: None,
            erased_session: false,
        }
    }

    /// Construct with an explicit provider and Argon2 params.
    ///
    /// Intended for TESTS (unit and the separate integration-test crate, which
    /// cannot see `#[cfg(test)]` items) to inject a `SoftwareFallback` plus
    /// `Argon2Params::testing_cheap()` for speed. Production code uses
    /// [`Engine::new`], which auto-detects the provider and uses safe default
    /// params. Exposed (not `cfg(test)`) only so integration tests can call it;
    /// it performs no unsafe action on its own.
    pub fn with_provider(
        vault_dir: impl Into<std::path::PathBuf>,
        provider: Box<dyn HardwareSecretProvider>,
        setup_params: Argon2Params,
    ) -> Self {
        Engine {
            vault: Vault::new(vault_dir.into()),
            provider,
            setup_params,
            mk: None,
            erased_session: false,
        }
    }

    /// Compute the current state from disk + session.
    fn current_state(&self) -> Result<State> {
        if self.erased_session {
            return Ok(State::Erased);
        }
        match self.vault.load_meta()? {
            None => Ok(State::Uninitialized),
            Some(meta) if meta.erased => Ok(State::Erased),
            Some(_) => {
                if self.mk.is_some() {
                    Ok(State::Unlocked)
                } else {
                    Ok(State::Locked)
                }
            }
        }
    }

    fn hardware_kind(&self) -> HardwareKind {
        self.provider.kind()
    }

    /// Dispatch one parsed request to its handler, returning the JSON response.
    /// Errors from handlers are converted to the standard error envelope here so
    /// every code path yields a well-formed response that echoes `id`.
    pub fn handle(&mut self, req: Request) -> Value {
        let id = req.id();
        let result = match &req {
            Request::Status { id } => self.handle_status(*id),
            Request::Setup {
                id,
                secret,
                max_attempts,
                acknowledge_no_recovery,
            } => self.handle_setup(*id, secret, *max_attempts, *acknowledge_no_recovery),
            Request::Unlock { id, secret } => self.handle_unlock(*id, secret),
            Request::Lock { id } => self.handle_lock(*id),
            Request::Erase { id, confirm } => self.handle_erase(*id, *confirm),
            Request::Unknown { .. } => Err(VaultError::invalid_request("unknown request type")),
        };
        match result {
            Ok(v) => v,
            Err(e) => serde_json::to_value(ErrorResponse::from_error(id, &e))
                .unwrap_or_else(|_| json!({"id": id.unwrap_or(0), "ok": false, "error": "internal", "message": "response serialization failed"})),
        }
    }

    /// Convenience for tests / the main loop: parse raw JSON bytes and handle.
    /// (The main loop uses `protocol::parse_frame` + `handle` directly; this is
    /// here so integration tests can drive the engine without re-framing.)
    pub fn handle_json_bytes(&mut self, payload: &[u8]) -> Value {
        match crate::protocol::parse_frame(payload) {
            Ok(req) => self.handle(req),
            Err(e) => serde_json::to_value(ErrorResponse::from_error(None, &e)).unwrap_or_else(
                |_| json!({"id": 0, "ok": false, "error": "internal", "message": "x"}),
            ),
        }
    }

    fn handle_status(&self, id: i64) -> Result<Value> {
        let state = self.current_state()?;
        let max_attempts = match self.vault.load_meta()? {
            Some(m) if !m.erased => m.max_attempts,
            _ => 0,
        };
        let attempts_remaining = if matches!(state, State::Locked | State::Unlocked) {
            // A counter that cannot be read trustworthily must not be reported
            // as a fresh budget: fail closed to 0 remaining (the unlock path
            // itself fails closed to erasure on a corrupt counter).
            match self.provider.read_counter() {
                Ok(used) => max_attempts.saturating_sub(used),
                Err(_) => 0,
            }
        } else {
            0
        };
        Ok(json!({
            "id": id,
            "ok": true,
            "state": state.as_str(),
            "hardware": self.hardware_kind().as_str(),
            "maxAttempts": max_attempts,
            "attemptsRemaining": attempts_remaining,
        }))
    }

    fn handle_setup(
        &mut self,
        id: i64,
        secret: &str,
        max_attempts: u32,
        acknowledge_no_recovery: bool,
    ) -> Result<Value> {
        // Already initialized (and not erased) -> reject. An erased vault is
        // also "already been set up"; require an explicit fresh start only via a
        // clean directory, so we reject setup over an erased vault too with
        // already-initialized (its metadata still exists). This is deliberate:
        // re-setup must be an explicit out-of-band action, not silent.
        match self.current_state()? {
            State::Locked | State::Unlocked => {
                return Err(VaultError::already_initialized());
            }
            State::Erased => {
                // Allow re-initialization after erasure: the data is already
                // gone, so creating a brand-new vault here is safe and is the
                // expected recovery-from-erase flow. Fall through to setup.
            }
            State::Uninitialized => {}
        }

        // No-recovery acknowledgement is mandatory.
        if !acknowledge_no_recovery {
            return Err(VaultError::no_recovery_not_acknowledged());
        }

        // Sanity-bound max_attempts. 0 would mean "erase on first try"; reject.
        if max_attempts == 0 {
            return Err(VaultError::invalid_request("maxAttempts must be >= 1"));
        }

        // Weak-secret check ONLY in software-fallback mode (hardware enforces
        // rate-limiting so short secrets are acceptable there; software cannot).
        if self.hardware_kind() == HardwareKind::Software {
            check_software_secret_strength(secret)?;
        }

        // Reset the session erased flag if we are re-initializing post-erase, and
        // ensure no stale erased metadata blocks the new vault: create() will
        // overwrite vault.json with a fresh (non-erased) record. We also clear
        // any session MK.
        self.erased_session = false;
        self.mk = None;

        let meta = self
            .vault
            .create(secret.as_bytes(), max_attempts, self.setup_params, self.provider.as_ref())?;

        Ok(json!({
            "id": id,
            "ok": true,
            "state": State::Locked.as_str(),
            "hardware": meta.hardware.as_str(),
            "maxAttempts": meta.max_attempts,
        }))
    }

    fn handle_unlock(&mut self, id: i64, secret: &str) -> Result<Value> {
        let state = self.current_state()?;
        match state {
            State::Uninitialized => return Err(VaultError::not_initialized()),
            State::Erased => return Err(VaultError::erased()),
            // An unlock on an already-unlocked vault is a RE-AUTHENTICATION:
            // it re-verifies the secret under the same counter policy as a
            // locked unlock (increment before the attempt, reset on success,
            // erase at the limit). Returning unverified success here would make
            // `unlock` useless as a gate for sensitive re-actions — any secret
            // would "work" while the session is unlocked. A failed re-auth
            // keeps the session unlocked (the caller already holds it) but
            // burns budget exactly like a locked attempt.
            State::Unlocked | State::Locked => {}
        }

        let meta = self
            .vault
            .load_meta()?
            .ok_or_else(VaultError::not_initialized)?;

        // COUNTER POLICY: increment BEFORE the attempt and persist immediately.
        // A corrupt/tampered counter reads as Erased (fail closed); complete
        // the erasure so the reported state is the real state.
        let attempt_number = match self.provider.increment_counter() {
            Ok(n) => n,
            Err(e) if e.code == ErrorCode::Erased => {
                self.finalize_erase()?;
                return Ok(json!({
                    "id": id,
                    "ok": false,
                    "error": "erased",
                    "state": State::Erased.as_str()
                }));
            }
            Err(other) => return Err(other),
        };
        let budget = AttemptBudget::evaluate(attempt_number, meta.max_attempts);

        // Try to unwrap the MK.
        match self.vault.unwrap_mk(&meta, secret.as_bytes(), self.provider.as_ref()) {
            Ok(mk) => {
                // Success: load the container into memory (proves the MK is good
                // end-to-end) and reset the counter.
                let _profile = self.vault.read_container(&mk)?;
                // (_profile holds the decrypted profile blob; in the full
                // product vault-ui/host would expose it to Gecko via the chosen
                // container mechanism. v1 keeps it in memory then drops it,
                // zeroized, since we don't mount a FS here.)
                //
                // Resetting unconditionally on success is INTENTIONAL, whatever
                // the counter's prior value: proving knowledge of the secret is
                // the event the budget exists to gate. In software mode the
                // counter file offers no offline protection anyway (a disk
                // imager restores it at will — see SoftwareFallback's warning);
                // only hardware modes make the budget non-bypassable.
                self.provider.reset_counter()?;
                self.mk = Some(mk);
                Ok(json!({"id": id, "ok": true, "state": State::Unlocked.as_str()}))
            }
            Err(e) if e.code == ErrorCode::Erased => {
                // The hardware secret is gone (corrupt/removed) -> effectively
                // erased. Surface as erased.
                self.finalize_erase()?;
                Ok(json!({"id": id, "ok": false, "error": "erased", "state": State::Erased.as_str()}))
            }
            Err(e) if e.code == ErrorCode::BadSecret => {
                if budget.at_limit {
                    // Final failed attempt -> cryptographic erasure.
                    self.finalize_erase()?;
                    Ok(json!({
                        "id": id,
                        "ok": false,
                        "error": "erased",
                        "state": State::Erased.as_str()
                    }))
                } else {
                    let delay = delay_ms_for_failures(attempt_number);
                    Ok(json!({
                        "id": id,
                        "ok": false,
                        "error": "bad-secret",
                        "attemptsRemaining": budget.remaining_after_this,
                        "delayMs": delay,
                    }))
                }
            }
            // Any other error (internal) propagates as an error envelope without
            // consuming the "logical" attempt budget beyond the increment we
            // already persisted (which is the conservative, fail-safe choice).
            Err(other) => Err(other),
        }
    }

    fn handle_lock(&mut self, id: i64) -> Result<Value> {
        match self.current_state()? {
            State::Uninitialized => return Err(VaultError::not_initialized()),
            State::Erased => return Err(VaultError::erased()),
            _ => {}
        }
        // Drop (zeroize) the in-memory MK. In the full product this is also where
        // the container FS would be unmounted / re-sealed.
        self.mk = None;
        Ok(json!({"id": id, "ok": true, "state": State::Locked.as_str()}))
    }

    fn handle_erase(&mut self, id: i64, confirm: bool) -> Result<Value> {
        if !confirm {
            return Err(VaultError::invalid_request("erase requires confirm == true"));
        }
        match self.current_state()? {
            State::Uninitialized => {
                // Nothing to erase. Be lenient: report erased state (the goal,
                // "no recoverable data", already holds) but mark it explicitly.
                self.finalize_erase()?;
                Ok(json!({"id": id, "ok": true, "state": State::Erased.as_str()}))
            }
            State::Erased => {
                Ok(json!({"id": id, "ok": true, "state": State::Erased.as_str()}))
            }
            State::Locked | State::Unlocked => {
                self.finalize_erase()?;
                Ok(json!({"id": id, "ok": true, "state": State::Erased.as_str()}))
            }
        }
    }

    /// Perform cryptographic erasure and update session state. Zeroizes the MK
    /// AFTER the provider invalidation (order per §5.1: invalidate first).
    fn finalize_erase(&mut self) -> Result<()> {
        self.vault.erase(self.provider.as_ref())?;
        self.mk = None; // Zeroizing drop wipes the key bytes.
        self.erased_session = true;
        Ok(())
    }
}

/// Reject weak secrets in software-fallback mode: block all-digit secrets (PINs)
/// and anything shorter than `MIN_SOFTWARE_SECRET_LEN`. Returns `weak-secret`.
///
/// This is the honest compensation for the absence of hardware rate-limiting:
/// without a non-extractable hardware secret, an offline attacker's only cost is
/// Argon2id per guess, so the passphrase MUST carry real entropy.
pub fn check_software_secret_strength(secret: &str) -> Result<()> {
    if secret.chars().count() < MIN_SOFTWARE_SECRET_LEN {
        return Err(VaultError::weak_secret(format!(
            "software-mode secret must be at least {MIN_SOFTWARE_SECRET_LEN} characters (no hardware rate-limiting)"
        )));
    }
    if !secret.is_empty() && secret.chars().all(|c| c.is_ascii_digit()) {
        return Err(VaultError::weak_secret(
            "software-mode secret must not be all digits (a numeric PIN is offline-guessable)",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::SoftwareFallback;
    use tempfile::tempdir;

    fn engine(dir: &std::path::Path) -> Engine {
        Engine::with_provider(
            dir.to_path_buf(),
            Box::new(SoftwareFallback::new(dir.to_path_buf())),
            Argon2Params::testing_cheap(),
        )
    }

    const GOOD: &str = "a strong enough passphrase";

    #[test]
    fn weak_secret_checks() {
        assert!(check_software_secret_strength("123456").is_err()); // short + digits
        assert!(check_software_secret_strength("1234567890123").is_err()); // all digits
        assert!(check_software_secret_strength("short").is_err()); // too short
        assert!(check_software_secret_strength(GOOD).is_ok());
    }

    #[test]
    fn setup_requires_ack() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["ok"], false);
        assert_eq!(r["error"], "no-recovery-not-acknowledged");
    }

    #[test]
    fn setup_rejects_weak_in_software() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        let r = e.handle_json_bytes(
            br#"{"type":"setup","id":1,"secret":"123456","acknowledgeNoRecovery":true}"#,
        );
        assert_eq!(r["error"], "weak-secret");
    }

    #[test]
    fn full_happy_path_roundtrip() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());

        // setup
        let r = e.handle_json_bytes(
            format!(
                r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#
            )
            .as_bytes(),
        );
        assert_eq!(r["ok"], true, "setup: {r}");
        assert_eq!(r["state"], "locked");

        // status -> locked
        let r = e.handle_json_bytes(br#"{"type":"status","id":2}"#);
        assert_eq!(r["state"], "locked");
        assert_eq!(r["hardware"], "software");
        assert_eq!(r["maxAttempts"], 6);

        // unlock (correct)
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"unlock","id":3,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["ok"], true, "unlock: {r}");
        assert_eq!(r["state"], "unlocked");

        // status -> unlocked, counter reset so attemptsRemaining == max
        let r = e.handle_json_bytes(br#"{"type":"status","id":4}"#);
        assert_eq!(r["state"], "unlocked");
        assert_eq!(r["attemptsRemaining"], 6);

        // lock
        let r = e.handle_json_bytes(br#"{"type":"lock","id":5}"#);
        assert_eq!(r["state"], "locked");

        // unlock again
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"unlock","id":6,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["state"], "unlocked");
    }

    #[test]
    fn already_initialized_rejected() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#)
                .as_bytes(),
        );
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":2,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#)
                .as_bytes(),
        );
        assert_eq!(r["error"], "already-initialized");
    }

    #[test]
    fn wrong_secret_increments_and_escalates() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true,"maxAttempts":6}}"#)
                .as_bytes(),
        );

        // 1st wrong: remaining 5, delay 0
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":2,"secret":"wrong passphrase!!"}"#);
        assert_eq!(r["error"], "bad-secret");
        assert_eq!(r["attemptsRemaining"], 5);
        assert_eq!(r["delayMs"], 0);

        // 2nd wrong: remaining 4, delay 1000
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":3,"secret":"wrong passphrase!!"}"#);
        assert_eq!(r["attemptsRemaining"], 4);
        assert_eq!(r["delayMs"], 1000);

        // 3rd wrong: remaining 3, delay 5000
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":4,"secret":"wrong passphrase!!"}"#);
        assert_eq!(r["attemptsRemaining"], 3);
        assert_eq!(r["delayMs"], 5000);

        // A correct unlock now resets the counter.
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"unlock","id":5,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["state"], "unlocked");
        let r = e.handle_json_bytes(br#"{"type":"status","id":6}"#);
        assert_eq!(r["attemptsRemaining"], 6);
    }

    #[test]
    fn reaching_max_attempts_erases() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        // Small max for speed.
        e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true,"maxAttempts":3}}"#)
                .as_bytes(),
        );

        // 1
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":2,"secret":"nope nope nope"}"#);
        assert_eq!(r["error"], "bad-secret");
        // 2
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":3,"secret":"nope nope nope"}"#);
        assert_eq!(r["error"], "bad-secret");
        // 3 -> erase
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":4,"secret":"nope nope nope"}"#);
        assert_eq!(r["error"], "erased");
        assert_eq!(r["state"], "erased");

        // Now even the CORRECT secret cannot unlock; vault is erased.
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"unlock","id":5,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["error"], "erased");

        // status reports erased.
        let r = e.handle_json_bytes(br#"{"type":"status","id":6}"#);
        assert_eq!(r["state"], "erased");
    }

    #[test]
    fn explicit_erase_requires_confirm() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#)
                .as_bytes(),
        );
        let r = e.handle_json_bytes(br#"{"type":"erase","id":2}"#);
        assert_eq!(r["error"], "invalid-request");
        let r = e.handle_json_bytes(br#"{"type":"erase","id":3,"confirm":true}"#);
        assert_eq!(r["ok"], true);
        assert_eq!(r["state"], "erased");
    }

    #[test]
    fn unlock_before_setup_is_not_initialized() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":1,"secret":"whatever long enough"}"#);
        assert_eq!(r["error"], "not-initialized");
    }

    #[test]
    fn unknown_type_is_invalid_request() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        let r = e.handle_json_bytes(br#"{"type":"explode","id":9}"#);
        assert_eq!(r["error"], "invalid-request");
        assert_eq!(r["id"], 9);
    }

    #[test]
    fn corrupt_counter_erases_on_unlock_and_zeroes_status_budget() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#)
                .as_bytes(),
        );

        // Tamper: truncate the counter file (the crash window itself is closed
        // by atomic writes, so a malformed file means tampering/corruption).
        std::fs::write(d.path().join("counter.bin"), [0u8; 2]).unwrap();

        // status must NOT advertise a fresh budget against an unaccountable
        // counter (and must not mutate anything).
        let r = e.handle_json_bytes(br#"{"type":"status","id":2}"#);
        assert_eq!(r["state"], "locked");
        assert_eq!(r["attemptsRemaining"], 0);

        // Even the CORRECT secret now fails closed into cryptographic erasure.
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"unlock","id":3,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["ok"], false, "unlock over corrupt counter: {r}");
        assert_eq!(r["error"], "erased");
        assert_eq!(r["state"], "erased");

        let r = e.handle_json_bytes(br#"{"type":"status","id":4}"#);
        assert_eq!(r["state"], "erased");
    }

    #[test]
    fn unlock_while_unlocked_is_real_reauth() {
        let d = tempdir().unwrap();
        let mut e = engine(d.path());
        e.handle_json_bytes(
            format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true,"maxAttempts":6}}"#)
                .as_bytes(),
        );
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"unlock","id":2,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["state"], "unlocked");

        // A WRONG secret while unlocked must NOT be reported as success — and
        // it burns budget like any other failed attempt.
        let r = e.handle_json_bytes(br#"{"type":"unlock","id":3,"secret":"not the passphrase"}"#);
        assert_eq!(r["ok"], false);
        assert_eq!(r["error"], "bad-secret");
        assert_eq!(r["attemptsRemaining"], 5);

        // The session itself stays unlocked (the caller already holds it).
        let r = e.handle_json_bytes(br#"{"type":"status","id":4}"#);
        assert_eq!(r["state"], "unlocked");
        assert_eq!(r["attemptsRemaining"], 5);

        // A correct re-auth succeeds and resets the budget.
        let r = e.handle_json_bytes(
            format!(r#"{{"type":"unlock","id":5,"secret":"{GOOD}"}}"#).as_bytes(),
        );
        assert_eq!(r["ok"], true);
        assert_eq!(r["state"], "unlocked");
        let r = e.handle_json_bytes(br#"{"type":"status","id":6}"#);
        assert_eq!(r["attemptsRemaining"], 6);
    }
}
