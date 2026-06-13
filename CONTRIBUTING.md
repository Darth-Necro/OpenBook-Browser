<!-- SPDX-License-Identifier: MPL-2.0 -->

# Contributing to OpenBook

OpenBook is a patch / settings / extension / native-host / release layer over a
pinned upstream Firefox stable — not a Gecko rewrite. Read
`docs/OpenBook-Browser-Build-Plan.md` (authoritative) and `CLAUDE.md`
(operational guardrails) before contributing. Log every non-trivial decision in
`docs/DECISIONS.md` as an ADR.

## Ground rules

1. **The security invariants are non-negotiable.** They are listed in
   `README.md` and `CLAUDE.md`. A change that violates one (telemetry, fail-open
   proxy fallback, deletion-as-erasure, unlabeled software lockout fallback,
   AI on by default, user-writable privileged files) will not be merged,
   regardless of other merits.
2. **Patch upstream, don't rewrite it.** Changes to Firefox behavior go in the
   ordered patch series (`patches/`), the AutoConfig layer
   (`config/autoconfig/`), or enterprise policy (`config/policies/`) — in that
   order of preference *reversed*: prefer prefs/policy, patch only what prefs
   and policy cannot reach.
3. **Destructive work runs in disposable environments.** Lockout, erasure,
   profile-encryption, and mount testing only ever runs in throwaway
   VMs/containers against throwaway profiles (invariant 7).
4. **No new outbound connections.** Anything that adds egress must be opt-in,
   documented, and covered by the privacy-regression suite.

## Repository layout

See `README.md` for the map. Per-component `README.md` files document as-built
detail.

## Development setup

- Python ≥ 3.9, Rust stable (with `clippy`), Node 22+, bash.
- No network is required for any offline gate.

```bash
# Python gates (structure, privacy regression, leak, repro)
python3 tests/phase0/test_phase0_structure.py
for t in tests/privacy-regression/test_*.py; do python3 "$t"; done
python3 tests/leak/failclosed_sim.py && python3 tests/leak/leak_assertions.py
python3 tests/repro/test_repro_diff.py

# Native hosts
cargo test --locked --manifest-path native/vault-host/Cargo.toml
cargo test --locked --manifest-path native/vpn-helper/Cargo.toml

# Extensions (repeat for proxy-manager, ai-sidebar)
npm --prefix extensions/vault-ui ci
npm --prefix extensions/vault-ui run build
npm --prefix extensions/vault-ui test
```

CI (`.github/workflows/ci.yml`) runs all of the above plus `clippy -D warnings`
and shell syntax checks on every push and pull request. Green CI is required to
merge.

## Making changes

- **Prefs / policy:** edit `config/autoconfig/openbook.cfg` (privileged JS —
  first line must stay a comment; values are JS literals) or
  `config/policies/policies.json`. Add or extend an assertion in
  `tests/privacy-regression/` for any hardening change; the suite is the
  regression contract.
- **Patches:** patches are an ordered series under `patches/<area>/`. Keep them
  minimal and rebaseable; `build/scripts/apply-patches.sh` must apply the whole
  series cleanly or fail — never hand-edit upstream trees.
- **Extensions:** TypeScript, Manifest V2 (ADR-0012), no remote SDKs, no new
  host permissions without an ADR. `npm run build && npm test` must pass.
- **Native hosts:** Rust; the native-messaging parser handles untrusted input —
  extend the protocol-robustness tests and keep fuzz targets building for any
  protocol change. No panics reachable from stdin.
- **Shell:** `bash -n` clean; fail closed (a missing tool/key/input aborts,
  never soft-continues).

## Commits and pull requests

- One logical change per commit; explain *why* in the body.
- Reference the Build-Plan section or ADR your change implements.
- New decisions of consequence get an ADR entry in `docs/DECISIONS.md`.
- Update `CHANGELOG.md` under the unreleased heading for user-visible changes.

## Security issues

Do **not** open public issues for vulnerabilities. See `docs/SECURITY.md` and
`.well-known/security.txt` for the disclosure process.
