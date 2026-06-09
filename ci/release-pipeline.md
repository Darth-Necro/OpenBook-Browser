<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook release pipeline (Build Plan §9)

This is the authoritative description of the end-to-end pipeline that turns a
pinned, verified upstream Firefox release into signed, reproducible OpenBook
artifacts. It is intentionally **fail-hard** at the trust-critical steps: a hash
mismatch, a patch conflict, or a missing signing key **stops the release**.

Pinned upstream: **Firefox 145.0.2** (the single pin lives in
`build/scripts/fetch-verify-upstream.sh` and `ci/mfsa-track.sh`).

## Stages

```
fetch -> verify (FAIL HARD) -> apply patches (FAIL on conflict) -> matrix build
      -> tests -> package -> sign -> publish (checksums + signatures + SBOM)
```

1. **Fetch.** `build/scripts/fetch-verify-upstream.sh` downloads the exact pinned
   `firefox-145.0.2.source.tar.xz` plus Mozilla's SHA-256 manifest and detached
   signature.
2. **Verify — FAIL HARD.** Verify the SHA-256 against Mozilla's manifest **and**
   the detached GPG signature against Mozilla's signing key. Any mismatch aborts
   the entire pipeline with a nonzero exit; nothing downstream runs. (Security
   invariant: source integrity is non-negotiable.)
3. **Apply patches — FAIL on conflict.** `build/scripts/apply-patches.sh` applies
   the ordered OpenBook patch series (branding / privacy / features). A patch that
   does not apply cleanly **stops the build and alerts** — we never force or
   partially apply a series (a half-applied privacy patch is a silent security
   regression).
4. **Matrix build.** `build/scripts/build.sh` runs `mach` per target with the
   pinned `mozconfig` inside the pinned container. The build **matrix** is:

   | Target            | Arch            | Build host / runner             |
   |-------------------|-----------------|---------------------------------|
   | `linux-x64`       | x86_64          | Linux (containerized, pinned)   |
   | `linux-arm64`     | aarch64         | Linux arm64 (native or emulated)|
   | `win-x64`         | x86_64          | Windows                         |
   | `macos-universal` | x86_64 + arm64  | macOS (universal binary)        |

   Frontend-only changes may use artifact builds; release builds are full,
   containerized, with pinned toolchains and `SOURCE_DATE_EPOCH` set for
   reproducibility.
5. **Tests.** Run the gates:
   - Native (Rust): `cargo test` for `native/vault-host` and `native/vpn-helper`
     (+ the fuzz/property targets where configured).
   - Extensions (TS): jest unit + web-ext/Marionette/Playwright-Firefox integration.
   - Privacy regression: telemetry-off, RFP-on, and the **first-run egress test**
     (fails on any unexpected outbound connection).
   - Leak suite: `tests/leak/failclosed_sim.py` and `tests/leak/leak_assertions.py`
     offline gates now; the live WebRTC/DNS/IPv6/tunnel-failure harness on a real
     build (see `tests/leak/README.md`).
   - Reproducible-build diff: rebuild in a clean container and
     `ci/repro-diff.sh REBUILD PUBLISHED` (see `tests/repro/README.md`).
6. **Package.** `build/scripts/package.sh --source DIR --target T --format F` runs
   `mach package` then the per-OS packager on the matching host. It **fails closed**
   if a packaging tool is missing. Privileged files (`openbook.cfg`,
   `defaults/pref/*.js`, the native host binary + manifest) MUST be packaged
   **root-owned and not user-writable** (§11) — a packaging-time invariant and a
   release blocker if violated.
7. **Sign.** `build/scripts/sign.sh --target T --artifact PATH`:
   - **Linux:** GPG detached signature + SHA-256 checksums; deb/rpm repos are
     GPG-signed at publish time.
   - **Windows:** Authenticode (signtool/osslsigncode), EV cert in HSM preferred.
   - **macOS:** `codesign` (Developer ID, hardened runtime) → `notarytool submit`
     → `stapler staple`.
   Keys live **only** in HSM / hardware tokens / platform signing services. The
   script **refuses (nonzero)** if the required key handle is unset — never embeds
   or generates a key. (§11 signing-key management.)
8. **Publish.** Upload artifacts with their **SHA-256 checksums**, **detached
   signatures**, and a **CycloneDX SBOM** (`ci/sbom.sh`). The SBOM is **required
   per release** (§11 supply chain).

## Patch maintenance & upstream security tracking (§9)

- **Rebase on upstream stable.** The patch series is rebased onto every new
  Firefox stable. CI tests patch application on each upstream release; a conflict
  is triaged immediately.
- **MFSA tracking.** `ci/mfsa-track.sh` reports Mozilla Foundation Security
  Advisories at/after the pin so maintainers always know the **CVE coverage gap**
  between the current build and upstream. Run it in CI (it degrades gracefully
  offline).
- **SLA.** Forks like LibreWolf run ~**1–2 days** behind upstream stable; that is
  the target SLA for rebuilding on a security release.

## Update distribution (§9 recommendation)

**Start with Option 2 — OS package managers + Flatpak / winget / Chocolatey**
(lowest operational burden; matches LibreWolf):

- **Linux:** `.deb`/`.rpm` from GPG-signed repos, Flatpak (own remote or Flathub),
  AppImage.
- **Windows:** signed installer + **winget** and/or **Chocolatey**; portable zip.
- **macOS:** signed+notarized `.dmg`/`.pkg` (Homebrew cask optional).

**Migrate later toward Option 1/3 — self-hosted Firefox-native updates (AUS/Balrog
with signed MAR files)** if/when security patches need to be pushed faster than
package channels allow (true in-browser auto-update; requires running update infra
and managing MAR signing keys in HSM). This is the v2 distribution track.

## Trust-critical fail-hard summary

| Step | On failure |
|---|---|
| Source hash/signature verify | **Abort the release** (nonzero). |
| Patch series apply | **Abort + alert** on any conflict (no partial apply). |
| Privacy/egress tests | **Fail the build** on any unexpected outbound connection. |
| Packaging prerequisite missing | **Fail closed** (no half-package). |
| Signing key handle missing | **Refuse** (nonzero); keys only in HSM. |
| Reproducible-build diff | **Fail** on MISMATCH; investigate before publish. |
