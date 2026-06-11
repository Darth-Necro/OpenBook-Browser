# OpenBook Browser

A cross-platform, free, open-source, privacy-hardened fork of **Firefox stable**, in the
LibreWolf / Mullvad Browser / Tor Browser model. OpenBook is **not** a rewrite of Gecko — it is a
patch, branding, settings, WebExtension, native-host, build, and release layer over a pinned and
cryptographically verified upstream Firefox source tarball.

> **Status:** pre-release, under active development. Builds, hardware-backed features, signing, and
> audits require real build hosts and hardware and are orchestrated by CI / release engineering —
> they are not run from a developer's interactive session. See `docs/OpenBook-Browser-Build-Plan.md`.

## What OpenBook is for

- **Zero telemetry.** No unsolicited outbound connections; CI includes a first-run egress test.
- **Privacy hardening** via Firefox AutoConfig (`openbook.cfg`) and enterprise policy (`policies.json`).
- **Cryptographic lockout** — hardware-bound, data-at-rest protection for a lost or stolen device,
  in the model of iOS Data Protection / OS full-disk encryption. "Erase" means cryptographic erasure
  (invalidate the key), never best-effort file deletion.
- **User-controlled proxy/VPN** with fail-closed leak protection (WebRTC / DNS / IPv6 / kill-switch).
- **Opt-in AI assistant**, **off by default**, local-first, with page content treated as untrusted
  and every action gated behind explicit per-action confirmation.

## Security invariants (non-negotiable)

These are enforced across all phases and are release blockers if violated. See `CLAUDE.md` and
`docs/SECURITY.md`.

1. Zero telemetry / no unsolicited egress (proven by patch + policy + CI egress test).
2. Proxy/VPN fails **closed** — tunnel loss blocks traffic, never silent direct fallback.
3. Erasure = cryptographic erasure (invalidate keys); deletion/overwrite is not the guarantee.
4. Lockout counters hardware-enforced (TPM 2.0 / Secure Enclave) where available; otherwise a strong
   Argon2id passphrase is required and the weaker guarantee is labeled.
5. AI is off by default — no provider, network, or telemetry until explicit opt-in.
6. Privileged files (`openbook.cfg`, `defaults/pref/*.js`, native host binary + manifest) are
   root-owned and not user-writable in release packages.
7. Destructive lockout / encryption / mount work runs only in disposable VMs/containers against
   throwaway profiles.

## Repository layout

```
build/       mozconfig/ scripts/ docker/        # fetch-verify, apply-patches, build, package, sign
patches/     branding/ privacy/ features/       # ordered, rebaseable patch series over upstream
branding/    openbook/                           # OpenBook identity (rebrand is mandatory — §13)
config/      autoconfig/ policies/ distribution/ # the settings + hardening layer
extensions/  vault-ui/ proxy-manager/ ai-sidebar/# bundled first-party WebExtensions (TypeScript)
native/      vault-host/ vpn-helper/             # Rust native messaging hosts (crypto/TPM/erasure)
ci/          sbom, repro-diff, MFSA tracking, pipeline docs
tests/       native/ extensions/ privacy-regression/ leak/ repro/ phase0/
docs/        Build-Plan, THREAT-MODEL, BUILD, SECURITY, DECISIONS (ADR log)
```

## Build & verify (quick start)

Full detail in `docs/BUILD.md`. Phase 0 static checks and the offline test gates run anywhere:

```bash
# Repo structure + shell syntax
python3 tests/phase0/test_phase0_structure.py
find build/scripts -type f -name '*.sh' -print0 | xargs -0 -n1 bash -n

# Privacy / leak / repro offline gates
python3 tests/privacy-regression/test_autoconfig_hardening.py
python3 tests/leak/failclosed_sim.py
python3 tests/repro/test_repro_diff.py

# Native host (Rust)
cargo test --manifest-path native/vault-host/Cargo.toml

# Extensions (TypeScript)
npm --prefix extensions/vault-ui ci && npm --prefix extensions/vault-ui test
```

Fetching, verifying, patching, and building actual Firefox happens on a suitable build host:

```bash
OPENBOOK_UPSTREAM_GPG_KEYRING=/path/to/mozilla-release.gpg \
  build/scripts/fetch-verify-upstream.sh --dest /tmp/openbook-upstream
build/scripts/apply-patches.sh --source /tmp/openbook-upstream/firefox-145.0.2
build/scripts/build.sh   --source /tmp/openbook-upstream/firefox-145.0.2 --target linux-x64
```

## Releases

Versioning is `<upstream-firefox>-<openbook-build>` (e.g. `145.0.2-1`; ADR-0017), with the
`VERSION` file as the source of truth. Pushing tag `v<VERSION>` runs the release workflow:
all gates re-run, and the component artifacts (extension XPIs, linux-x64 native hosts, the
settings overlay tarball, CycloneDX SBOM, `SHA256SUMS`) are assembled deterministically into
a **draft** GitHub release. Signing happens only on maintainer hardware (`build/scripts/sign.sh`),
and the draft is published only after every box in `docs/RELEASE-CHECKLIST.md` is checked —
including full per-OS browser builds, the live first-run egress test, the leak harness, the
lockout acceptance run, the permissions-invariant audit, and the reproducible-build diff.

To verify a published release: check `SHA256SUMS.asc` against the OpenBook release key, then
`sha256sum -c SHA256SUMS --ignore-missing`. See `CHANGELOG.md` for what shipped.

## License

OpenBook's own code is licensed **MPL-2.0** (see `LICENSE`), matching upstream Firefox to ease patch
interchange. Upstream Firefox code remains under its own MPL-2.0 terms. The Firefox name and logo are
Mozilla trademarks; OpenBook ships its own branding (`branding/`) and no Firefox marks.

## Security reporting

See `docs/SECURITY.md` and `.well-known/security.txt`. Do not file public issues for vulnerabilities.

## Funding posture

Independence first: a nonprofit / donations / grants model (no ad/search/telemetry monetization),
recorded in `docs/DECISIONS.md` (ADR-0004). This protects the zero-telemetry invariant.
