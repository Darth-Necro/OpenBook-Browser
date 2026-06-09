// SPDX-License-Identifier: MPL-2.0
//
// libFuzzer target for the vault host frame parser (Build Plan §5.4).
//
// Contract under test: `parse_frame` must NEVER panic on arbitrary bytes, must
// never allocate more than its input, and must return either a typed `Request`
// or an `invalid-request` error. libFuzzer feeds arbitrary `data`; any panic
// (including allocation/overflow aborts) is a finding.
//
// Run:  cargo +nightly fuzz run parse_frame
#![no_main]

use libfuzzer_sys::fuzz_target;
use openbook_vault_host::protocol::parse_frame;

fuzz_target!(|data: &[u8]| {
    // The result is intentionally ignored: we only care that this does not
    // panic / abort. Both Ok and Err are valid outcomes for arbitrary input.
    let _ = parse_frame(data);
});
