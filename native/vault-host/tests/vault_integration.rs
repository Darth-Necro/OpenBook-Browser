// SPDX-License-Identifier: MPL-2.0
//
// Integration tests for the OpenBook vault host (Build Plan §5.4 acceptance).
//
// These drive the public library surface (Engine + protocol) end-to-end against
// a TEMPDIR — never a real profile, never the dev host's data. They cover the
// Phase 2 acceptance criteria:
//   * full setup -> unlock -> lock -> unlock -> status round-trip,
//   * wrong-secret increments the counter and returns escalating delays,
//   * the counter PERSISTS across a simulated process restart (re-open vault),
//   * reaching maxAttempts triggers cryptographic erasure and the vault becomes
//     PERMANENTLY undecryptable (the MK cannot be recovered even with the right
//     secret),
//   * explicit erase.
//
// We build engines with the SOFTWARE fallback and CHEAP Argon2 params via the
// `testing` accessor so the suite is fast. (Production uses 64 MiB / t=3.)

use openbook_vault_host::engine::Engine;
use serde_json::Value;

const GOOD: &str = "a strong enough passphrase";

/// Build an engine over `dir` using cheap test params. Mirrors the helper in the
/// engine unit tests but via the public test constructor.
fn engine(dir: &std::path::Path) -> Engine {
    Engine::with_provider(
        dir.to_path_buf(),
        Box::new(openbook_vault_host::hardware::SoftwareFallback::new(
            dir.to_path_buf(),
        )),
        openbook_vault_host::kdf::Argon2Params::testing_cheap(),
    )
}

fn req(e: &mut Engine, json: &str) -> Value {
    e.handle_json_bytes(json.as_bytes())
}

#[test]
fn full_roundtrip_setup_unlock_lock_unlock_status() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());

    let r = req(
        &mut e,
        &format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#),
    );
    assert_eq!(r["ok"], true, "setup failed: {r}");
    assert_eq!(r["state"], "locked");
    assert_eq!(r["hardware"], "software");

    let r = req(&mut e, r#"{"type":"unlock","id":2,"secret":"a strong enough passphrase"}"#);
    assert_eq!(r["ok"], true);
    assert_eq!(r["state"], "unlocked");

    let r = req(&mut e, r#"{"type":"lock","id":3}"#);
    assert_eq!(r["state"], "locked");

    let r = req(&mut e, r#"{"type":"unlock","id":4,"secret":"a strong enough passphrase"}"#);
    assert_eq!(r["state"], "unlocked");

    let r = req(&mut e, r#"{"type":"status","id":5}"#);
    assert_eq!(r["state"], "unlocked");
    assert_eq!(r["maxAttempts"], 6);
    assert_eq!(r["attemptsRemaining"], 6); // reset on the successful unlock
}

#[test]
fn wrong_secret_increments_and_escalates() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());
    req(
        &mut e,
        &format!(
            r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true,"maxAttempts":6}}"#
        ),
    );

    let r = req(&mut e, r#"{"type":"unlock","id":2,"secret":"wrong passphrase one"}"#);
    assert_eq!(r["error"], "bad-secret");
    assert_eq!(r["attemptsRemaining"], 5);
    assert_eq!(r["delayMs"], 0);

    let r = req(&mut e, r#"{"type":"unlock","id":3,"secret":"wrong passphrase two"}"#);
    assert_eq!(r["attemptsRemaining"], 4);
    assert_eq!(r["delayMs"], 1000);

    let r = req(&mut e, r#"{"type":"unlock","id":4,"secret":"wrong passphrase three"}"#);
    assert_eq!(r["attemptsRemaining"], 3);
    assert_eq!(r["delayMs"], 5000);
}

#[test]
fn counter_persists_across_process_restart() {
    let dir = tempfile::tempdir().unwrap();

    // Process 1: set up, then make two wrong attempts.
    {
        let mut e = engine(dir.path());
        req(
            &mut e,
            &format!(
                r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true,"maxAttempts":6}}"#
            ),
        );
        req(&mut e, r#"{"type":"unlock","id":2,"secret":"bad attempt one!!"}"#);
        let r = req(&mut e, r#"{"type":"unlock","id":3,"secret":"bad attempt two!!"}"#);
        assert_eq!(r["attemptsRemaining"], 4);
    }

    // Process 2: a FRESH engine over the same dir must see the persisted counter
    // (power-cycling cannot reset it). status should report attemptsRemaining=4.
    {
        let mut e = engine(dir.path());
        let r = req(&mut e, r#"{"type":"status","id":4}"#);
        assert_eq!(r["state"], "locked");
        assert_eq!(r["attemptsRemaining"], 4, "counter must survive restart");

        // A third wrong attempt continues from 4 -> 3, not from a reset.
        let r = req(&mut e, r#"{"type":"unlock","id":5,"secret":"bad attempt three"}"#);
        assert_eq!(r["attemptsRemaining"], 3);
    }
}

#[test]
fn reaching_max_attempts_erases_permanently() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());
    req(
        &mut e,
        &format!(
            r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true,"maxAttempts":3}}"#
        ),
    );

    req(&mut e, r#"{"type":"unlock","id":2,"secret":"nope one nope one"}"#);
    req(&mut e, r#"{"type":"unlock","id":3,"secret":"nope two nope two"}"#);
    let r = req(&mut e, r#"{"type":"unlock","id":4,"secret":"nope three nope th"}"#);
    assert_eq!(r["error"], "erased");
    assert_eq!(r["state"], "erased");

    // The CORRECT secret no longer works: the MK is cryptographically gone.
    let r = req(&mut e, r#"{"type":"unlock","id":5,"secret":"a strong enough passphrase"}"#);
    assert_eq!(r["error"], "erased");

    // status reports erased even after a "restart".
    let mut e2 = engine(dir.path());
    let r = req(&mut e2, r#"{"type":"status","id":6}"#);
    assert_eq!(r["state"], "erased");
    let r = req(&mut e2, r#"{"type":"unlock","id":7,"secret":"a strong enough passphrase"}"#);
    assert_eq!(r["error"], "erased");
}

#[test]
fn erasure_destroys_on_disk_key_material() {
    // Stronger assertion: after erasure the on-disk metadata no longer contains
    // the wrapped-MK ciphertext and the container file is gone.
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());
    req(
        &mut e,
        &format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#),
    );

    let meta_path = dir.path().join("vault.json");
    let container_path = dir.path().join("container.enc");
    assert!(meta_path.exists());
    assert!(container_path.exists());
    let before = std::fs::read_to_string(&meta_path).unwrap();
    assert!(before.contains("wrappedMkHex") || before.contains("wrapped_mk_hex"));

    // Explicit erase.
    let r = req(&mut e, r#"{"type":"erase","id":2,"confirm":true}"#);
    assert_eq!(r["state"], "erased");

    // Container ciphertext deleted; metadata's wrapped MK scrubbed.
    assert!(!container_path.exists(), "container must be deleted");
    let after = std::fs::read_to_string(&meta_path).unwrap();
    let meta: Value = serde_json::from_str(&after).unwrap();
    assert_eq!(meta["erased"], true);
    assert_eq!(
        meta["wrappedMkHex"].as_str().unwrap_or(""),
        "",
        "wrapped MK ciphertext must be scrubbed from metadata"
    );
}

#[test]
fn explicit_erase_requires_confirm() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());
    req(
        &mut e,
        &format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#),
    );
    let r = req(&mut e, r#"{"type":"erase","id":2}"#);
    assert_eq!(r["error"], "invalid-request");
    let r = req(&mut e, r#"{"type":"erase","id":3,"confirm":true}"#);
    assert_eq!(r["ok"], true);
}

#[test]
fn weak_secret_rejected_in_software_mode() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());
    // All-digit PIN.
    let r = req(
        &mut e,
        r#"{"type":"setup","id":1,"secret":"123456","acknowledgeNoRecovery":true}"#,
    );
    assert_eq!(r["error"], "weak-secret");
    // Short non-digit.
    let r = req(
        &mut e,
        r#"{"type":"setup","id":2,"secret":"short","acknowledgeNoRecovery":true}"#,
    );
    assert_eq!(r["error"], "weak-secret");
}

#[test]
fn setup_requires_no_recovery_ack() {
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());
    let r = req(
        &mut e,
        &format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}"}}"#),
    );
    assert_eq!(r["error"], "no-recovery-not-acknowledged");
}

#[test]
fn re_setup_after_erase_is_allowed() {
    // After an erase the data is gone, so re-initializing a fresh vault is the
    // expected recovery flow and must succeed.
    let dir = tempfile::tempdir().unwrap();
    let mut e = engine(dir.path());
    req(
        &mut e,
        &format!(r#"{{"type":"setup","id":1,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#),
    );
    req(&mut e, r#"{"type":"erase","id":2,"confirm":true}"#);
    let r = req(
        &mut e,
        &format!(r#"{{"type":"setup","id":3,"secret":"{GOOD}","acknowledgeNoRecovery":true}}"#),
    );
    assert_eq!(r["ok"], true, "re-setup after erase should succeed: {r}");
    assert_eq!(r["state"], "locked");

    // And the fresh vault unlocks.
    let r = req(&mut e, r#"{"type":"unlock","id":4,"secret":"a strong enough passphrase"}"#);
    assert_eq!(r["state"], "unlocked");
}
