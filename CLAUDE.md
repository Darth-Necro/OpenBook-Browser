# OpenBook Browser

Operational guardrails for this repository. Full detail lives in `docs/OpenBook-Browser-Build-Plan.md`. Log every non-trivial decision in `docs/DECISIONS.md`.

## What this project is

OpenBook is a cross-platform, free, open-source, privacy-hardened fork of Firefox stable in the LibreWolf / Mullvad / Tor model. It is a patch, branding, settings, WebExtension, native-host, build, and release repository layered over a pinned upstream Firefox source tarball. Do not rewrite Gecko; patch upstream Firefox.

## Security invariants

1. Zero telemetry: no unsolicited outbound connections; CI must include first-run egress tests.
2. Fail closed: proxy/VPN tunnel loss blocks traffic; it never silently falls back to direct.
3. Erasure means cryptographic erasure by invalidating keys; file deletion or overwrite is not the privacy guarantee.
4. Lockout counters must be hardware-enforced where TPM 2.0 / Secure Enclave is available. Without hardware, require a strong passphrase via Argon2id and label the weaker guarantee.
5. AI is off by default: no provider, network calls, or telemetry until explicit opt-in; page content is untrusted and action-taking requires per-action confirmation.
6. Privileged files (`openbook.cfg`, `defaults/pref/*.js`, native host binary and manifest) must be root-owned and not user-writable in release packages.
7. Destructive lockout, profile-encryption, and filesystem-mount work runs only in disposable VMs/containers against throwaway profiles.

## Authoritative mechanisms

- Source integrity: pin an exact Firefox stable source release and verify Mozilla SHA256 manifests and detached signatures before patching.
- Build: `mach` with per-platform `mozconfig` files.
- Preference hardening: Firefox AutoConfig (`defaults/pref/autoconfig.js` and install-root `openbook.cfg`).
- Policy controls: Mozilla enterprise `distribution/policies.json`.
- UI features: bundled first-party WebExtensions.
- Crypto / TPM / Secure Enclave / erasure: Rust native messaging host using length-prefixed JSON over stdio.

## Repository map

See `docs/OpenBook-Browser-Build-Plan.md` for the complete tree and phased gates.
