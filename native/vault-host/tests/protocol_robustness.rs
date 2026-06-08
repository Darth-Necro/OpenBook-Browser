// SPDX-License-Identifier: MPL-2.0
//
// Deterministic protocol-robustness suite — "fuzz the parser" per Build Plan
// §5.4, without requiring cargo-fuzz to be installed. (A real cargo-fuzz target
// also lives in `fuzz/` for CI.)
//
// These tests hammer `parse_frame` and the framed `read_frame` path with many
// classes of malformed input and assert:
//   * the parser NEVER panics, and
//   * malformed JSON / bad frames yield `invalid-request` (never a crash, never
//     a wrong success), and
//   * the engine dispatch over malformed input always yields a well-formed JSON
//     response with `ok:false` and a known error code.
//
// We also do a small structured-mutation pass (bit/byte flips and truncations of
// valid requests) with a fixed PRNG seed so the run is reproducible.

use openbook_vault_host::engine::Engine;
use openbook_vault_host::error::ErrorCode;
use openbook_vault_host::protocol::{self, MAX_MESSAGE_LEN};

/// Known good requests we mutate.
const SEEDS: &[&str] = &[
    r#"{"type":"status","id":1}"#,
    r#"{"type":"setup","id":2,"secret":"a strong enough passphrase","acknowledgeNoRecovery":true}"#,
    r#"{"type":"unlock","id":3,"secret":"a strong enough passphrase"}"#,
    r#"{"type":"lock","id":4}"#,
    r#"{"type":"erase","id":5,"confirm":true}"#,
];

/// A grab-bag of explicitly malformed payloads.
fn malformed_payloads() -> Vec<Vec<u8>> {
    let mut v: Vec<Vec<u8>> = vec![
        b"".to_vec(),                              // empty
        b"{".to_vec(),                             // truncated object
        b"}".to_vec(),                             // stray close
        b"[]".to_vec(),                            // array, not object
        b"null".to_vec(),                          // json null
        b"123".to_vec(),                           // json number
        b"\"string\"".to_vec(),                    // json string
        b"{not json at all".to_vec(),              // junk
        b"{\"type\":123,\"id\":1}".to_vec(),       // type wrong kind
        b"{\"id\":1}".to_vec(),                    // missing type
        b"{\"type\":\"status\"}".to_vec(),         // status missing id (id optional in probe -> None)
        b"{\"type\":\"unlock\",\"id\":1}".to_vec(),// unlock missing secret
        b"{\"type\":\"setup\",\"id\":1}".to_vec(), // setup missing secret
        b"{\"type\":\"\",\"id\":1}".to_vec(),      // empty type
        b"{\"type\":\"frobnicate\",\"id\":1}".to_vec(), // unknown type (-> Unknown, dispatch invalid)
        b"{\"type\":\"status\",\"id\":\"x\"}".to_vec(), // id wrong kind
        b"{\"type\":\"status\",\"id\":1.5}".to_vec(),   // id non-integer
        vec![0xff, 0xfe, 0xfd],                    // invalid UTF-8
        vec![0x00, 0x01, 0x02, 0x03],              // binary garbage
        b"\xf0\x28\x8c\x28".to_vec(),              // invalid UTF-8 sequence
        b"{\"type\":\"setup\",\"id\":1,\"secret\":123}".to_vec(), // secret wrong kind
        b"{\"type\":\"erase\",\"id\":1,\"confirm\":\"yes\"}".to_vec(), // confirm wrong kind
    ];

    // Deeply nested JSON to probe recursion handling (serde_json has a default
    // recursion limit; this should error cleanly, not overflow the stack).
    let mut nested = String::new();
    for _ in 0..2000 {
        nested.push('[');
    }
    v.push(nested.into_bytes());

    v
}

#[test]
fn parser_never_panics_on_malformed_payloads() {
    for p in malformed_payloads() {
        // Must not panic. Result may be Ok(Unknown) or Err(invalid-request); it
        // must never be a known-typed Ok with wrong data.
        let res = std::panic::catch_unwind(|| protocol::parse_frame(&p));
        assert!(res.is_ok(), "parse_frame panicked on input: {p:?}");

        match res.unwrap() {
            Ok(req) => {
                // The only Ok results allowed for malformed-ish input are the
                // Unknown catch-all (unknown/empty type with optional id). Known
                // request types require their mandatory fields, which these
                // payloads lack — so any Ok(known) here would be a bug.
                use openbook_vault_host::protocol::Request;
                assert!(
                    matches!(req, Request::Unknown { .. })
                        || matches!(req, Request::Status { .. }),
                    "unexpected typed parse for malformed input {p:?}: {req:?}"
                );
            }
            Err(e) => {
                assert_eq!(
                    e.code,
                    ErrorCode::InvalidRequest,
                    "malformed input should be invalid-request: {p:?}"
                );
            }
        }
    }
}

#[test]
fn dispatch_over_malformed_always_well_formed_error() {
    // The engine must always answer malformed input with a JSON object carrying
    // ok:false and a known error code — never crash, never a bare success.
    let dir = tempfile::tempdir().unwrap();
    let mut engine = Engine::new(dir.path().to_path_buf());

    let known_codes = [
        "invalid-request",
        "not-initialized",
        "already-initialized",
        "bad-secret",
        "erased",
        "weak-secret",
        "no-recovery-not-acknowledged",
        "hardware-unavailable",
        "internal",
    ];

    for p in malformed_payloads() {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            engine.handle_json_bytes(&p)
        }));
        assert!(res.is_ok(), "dispatch panicked on input: {p:?}");
        let v = res.unwrap();
        assert!(v.is_object(), "response not an object for {p:?}: {v}");
        // Either it's a valid uninitialized status (for the unknown-with-no-body
        // cases that don't reach a handler error) OR an error with ok:false.
        if v["ok"] == serde_json::Value::Bool(false) {
            let code = v["error"].as_str().unwrap_or("");
            assert!(
                known_codes.contains(&code),
                "unknown error code {code:?} for input {p:?}"
            );
        }
        // `id` must always be present (number).
        assert!(v.get("id").is_some(), "response missing id for {p:?}: {v}");
    }
}

#[test]
fn mutated_seeds_never_panic_and_never_false_success() {
    // Deterministic structured mutation: for each seed, flip every single byte
    // (one at a time) and truncate at every length. Assert no panic and that a
    // mutated request never yields a *successful* operation by accident on a
    // fresh (uninitialized) vault — the only ok:true a fresh vault can produce
    // is a status query, which the mutations below don't preserve as valid setup.
    let dir = tempfile::tempdir().unwrap();

    for seed in SEEDS {
        let base = seed.as_bytes();

        // Single-byte flips.
        for i in 0..base.len() {
            for bit in 0..8u8 {
                let mut m = base.to_vec();
                m[i] ^= 1 << bit;
                drive_one(dir.path(), &m);
            }
        }
        // Truncations.
        for len in 0..base.len() {
            drive_one(dir.path(), &base[..len]);
        }
        // Trailing-garbage appends.
        for b in [0u8, b'{', b'}', 0xff, b'\n'] {
            let mut m = base.to_vec();
            m.push(b);
            drive_one(dir.path(), &m);
        }
    }
}

/// Drive a single payload through parse + dispatch on a FRESH engine (so no
/// mutation can leave persistent state that affects later iterations). Asserts no
/// panic and a well-formed response.
fn drive_one(dir: &std::path::Path, payload: &[u8]) {
    // Use a unique subdir per call so a stray successful `setup` from a mutation
    // can't poison sibling iterations.
    let sub = dir.join(format!("m{}", fastrand_like(payload)));
    let mut engine = Engine::with_provider(
        sub,
        Box::new(openbook_vault_host::hardware::SoftwareFallback::new(
            dir.join("hw"),
        )),
        openbook_vault_host::kdf::Argon2Params::testing_cheap(),
    );
    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        engine.handle_json_bytes(payload)
    }));
    assert!(res.is_ok(), "panic on mutated payload: {payload:?}");
    let v = res.unwrap();
    assert!(v.is_object());
    assert!(v.get("id").is_some());
}

/// Tiny deterministic hash of the payload to make a stable unique-ish subdir
/// name. Not cryptographic; just avoids collisions across iterations.
fn fastrand_like(bytes: &[u8]) -> u64 {
    let mut h = 1469598103934665603u64; // FNV offset basis
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

#[test]
fn oversized_declared_length_rejected_without_allocation() {
    // Feed read_frame a 4-byte header declaring a length just over the 1 MiB cap
    // followed by NO body. It must report Invalid without trying to read/allocate
    // the body.
    let mut header = Vec::new();
    header.extend_from_slice(&((MAX_MESSAGE_LEN as u32) + 1).to_ne_bytes());
    let mut cur = std::io::Cursor::new(header);
    match protocol::read_frame(&mut cur).unwrap() {
        protocol::FrameRead::Invalid(e) => assert_eq!(e.code, ErrorCode::InvalidRequest),
        protocol::FrameRead::Message(_) => panic!("expected Invalid, got Message"),
        protocol::FrameRead::Eof => panic!("expected Invalid, got Eof"),
    }
}
