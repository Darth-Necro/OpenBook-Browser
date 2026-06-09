// SPDX-License-Identifier: MPL-2.0
//
// libFuzzer target for the full parse + dispatch path (Build Plan §5.4).
//
// Beyond `parse_frame`, this exercises `Engine::handle_json_bytes`, so the state
// machine + handlers are fuzzed too. The engine is built ONCE over a temp dir
// (created at first call) so we don't thrash the filesystem per input; arbitrary
// fuzz bytes drive arbitrary request sequences against persisted vault state.
//
// Any panic is a finding: the host must always answer with a well-formed JSON
// response and never crash. We deliberately use cheap Argon2 params so the
// fuzzer makes progress (real params are intentionally slow).
//
// Run:  cargo +nightly fuzz run dispatch
#![no_main]

use std::sync::OnceLock;
use std::sync::Mutex;

use libfuzzer_sys::fuzz_target;
use openbook_vault_host::engine::Engine;
use openbook_vault_host::hardware::SoftwareFallback;
use openbook_vault_host::kdf::Argon2Params;

// A single engine + its backing tempdir, initialized once. The tempdir is kept
// alive for the process lifetime via the static.
static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();
static TMP: OnceLock<tempfile::TempDir> = OnceLock::new();

fn engine() -> &'static Mutex<Engine> {
    ENGINE.get_or_init(|| {
        let dir = TMP.get_or_init(|| tempfile::tempdir().expect("tempdir"));
        let e = Engine::with_provider(
            dir.path().to_path_buf(),
            Box::new(SoftwareFallback::new(dir.path().to_path_buf())),
            Argon2Params::testing_cheap(),
        );
        Mutex::new(e)
    })
}

fuzz_target!(|data: &[u8]| {
    let mut guard = engine().lock().unwrap();
    // Must not panic; return value is irrelevant to the fuzzer.
    let _ = guard.handle_json_bytes(data);
});
