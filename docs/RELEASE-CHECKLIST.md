<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook release checklist

The human gate for cutting a release. Pipeline detail lives in
`ci/release-pipeline.md`; this is the ordered checklist a release manager walks
through. Every box is a blocker — there is no "ship anyway."

Versioning (ADR-0017): `<upstream-firefox-version>-<openbook-build>`, e.g.
`145.0.2-1`; tag `v145.0.2-1`. The single source of truth is the `VERSION`
file; the release workflow refuses a tag that does not match it.

## 1. Decide and pin

- [ ] Upstream pin is current: check `ci/mfsa-track.sh` output; any MFSA at or
      after the pinned version that is fixed upstream means **re-pin first**
      (SLA: ~1–2 days behind upstream stable on security releases).
- [ ] `FIREFOX_VERSION` in `build/scripts/fetch-verify-upstream.sh`,
      `ci/mfsa-track.sh`, the `VERSION` file prefix, and
      `ci/release-pipeline.md` all agree.
- [ ] `VERSION` bumped (`-<n+1>` for a rebuild over the same upstream; reset to
      `-1` on a new upstream).
- [ ] `CHANGELOG.md` has a section for this exact version.
- [ ] New decisions since the last release are recorded in `docs/DECISIONS.md`.

## 2. Offline gates (anywhere)

- [ ] `python3 tests/phase0/test_phase0_structure.py`
- [ ] All `tests/privacy-regression/test_*.py` green.
- [ ] `python3 tests/leak/failclosed_sim.py` and `tests/leak/leak_assertions.py` green.
- [ ] `python3 tests/repro/test_repro_diff.py` green.
- [ ] `cargo test --locked` + `cargo clippy --locked -- -D warnings` for
      `native/vault-host` and `native/vpn-helper`.
- [ ] `npm ci && npm run build && npm test` for all three extensions.
- [ ] `bash -n` clean for `build/scripts/*.sh` and `ci/*.sh`.

## 3. Full builds (per-OS build hosts — not laptops, not this repo's CI)

- [ ] Fetch + verify upstream: `build/scripts/fetch-verify-upstream.sh`
      (Mozilla SHA-256 manifest **and** detached signature verified; any
      mismatch aborts the release).
- [ ] Patch series applies cleanly: `build/scripts/apply-patches.sh` (a
      conflict aborts — never force or partially apply).
- [ ] `build/scripts/build.sh` for `linux-x64`, `win-x64`, `macos-universal`
      (full builds, pinned containers, `SOURCE_DATE_EPOCH` set).
- [ ] Live first-run egress test on each built browser
      (`tests/privacy-regression/FIRST_RUN_EGRESS.md`): **zero** unexpected
      outbound connections.
- [ ] Live leak harness on a built browser (`tests/leak/README.md`): WebRTC,
      DNS, IPv6, and tunnel-failure vectors all hold, including fail-closed on
      proxy drop.
- [ ] Lockout acceptance in the disposable VM harness (never on a host):
      N failures → data verifiably unrecoverable; counter survives
      power-cycle; software fallback labeled.

## 4. Package

- [ ] `build/scripts/package.sh` per target/format (deb, rpm, flatpak,
      appimage, tar.xz; dmg, pkg; exe, msi).
- [ ] **Permissions invariant audit** on every package:
      `build/scripts/verify-release-permissions.sh` — `openbook.cfg`,
      `defaults/pref/*.js`, native host binary + manifest are root-owned and
      not user-writable. A violation is a release blocker (§11).
- [ ] Component artifacts assembled: `build/scripts/package-components.sh`
      (extension XPIs, native-host binaries + manifests, settings overlay
      bundle, SHA256SUMS) — this is what the tag-driven release workflow
      produces automatically as a draft.

## 5. Reproducibility, SBOM, signing

- [ ] Independent rebuild in a clean container; `ci/repro-diff.sh REBUILD
      PUBLISHED` reports MATCH (a MISMATCH is investigated before anything is
      published).
- [ ] SBOM generated with **no warnings**: `ci/sbom.sh` (CycloneDX for both
      Rust hosts and all three extensions).
- [ ] Sign on maintainer hardware — keys only in HSM / hardware tokens, never
      in CI (§11): `build/scripts/sign.sh --target <t> --artifact <path>` per
      artifact (Linux GPG detached + sha256; Windows Authenticode; macOS
      codesign → notarize → staple).
- [ ] `SHA256SUMS` regenerated over the final artifact set and itself
      GPG-signed (`SHA256SUMS.asc`).

## 6. Publish

- [ ] Tag `v<VERSION>` pushed; the release workflow's draft release is green
      (gates re-ran on the tag; artifacts + SBOM + checksums attached).
- [ ] Signed artifacts and `.asc` signatures uploaded to the draft.
- [ ] Release notes pasted from `CHANGELOG.md`, including the upstream Firefox
      version and the MFSA coverage statement from `ci/mfsa-track.sh`.
- [ ] Verification instructions present (how users check `SHA256SUMS.asc` and
      per-artifact signatures against the published release key).
- [ ] Draft flipped to published only after all of the above.

## 7. Post-release

- [ ] Package channels updated (deb/rpm repos re-signed, Flatpak remote,
      winget/Chocolatey manifests, Homebrew cask) per `ci/release-pipeline.md`.
- [ ] `ci/mfsa-track.sh` re-run; next-release tracking issue opened with any
      already-known gap.
- [ ] `CHANGELOG.md` gets a fresh unreleased heading; `VERSION` stays until
      the next cut.
