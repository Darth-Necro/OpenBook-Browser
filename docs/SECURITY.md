# Security Policy

OpenBook is a privacy/security project; this policy is part of the product. It is consistent with the
invariants in `CLAUDE.md`, the threat model in `docs/THREAT-MODEL.md`, and Build-Plan §11.

## Reporting vulnerabilities

Report privately — **do not** open public issues for security vulnerabilities. See
`.well-known/security.txt` (RFC 9116) for the current contact. Until a permanent disclosure channel is
published, the `Contact:` value is a placeholder and must be set to a monitored, ideally PGP-encrypted
channel before any stable release. Please allow coordinated-disclosure time before publishing details.

A vulnerability-disclosure policy and bug-bounty program are planned (Build-Plan §11, Phase 6).

## Release-blocking security requirements

A release is blocked if any of these is not satisfied:

1. **Source integrity.** Upstream Firefox source is hash- and signature-verified before patching or
   building; patch application fails hard on conflict.
2. **Zero telemetry.** No unsolicited telemetry or first-run network egress — proven by the AutoConfig
   locks, the `policies.json` duplicates, and the CI first-run egress test.
3. **Fail-closed networking.** Proxy/VPN behavior blocks traffic on tunnel loss; never silent direct
   fallback. The leak suite passes on WebRTC/DNS/IPv6/fail-open.
4. **Cryptographic erasure.** "Erase" invalidates keys (instantaneous crypto-erasure); deletion or
   overwrite is never relied upon (NIST SP 800-88).
5. **Hardware-enforced lockout where available.** TPM 2.0 / Secure Enclave enforces the attempt
   counter; the no-hardware fallback requires a strong Argon2id passphrase and is labeled weaker.
6. **Permissions invariant.** `openbook.cfg`, `defaults/pref/*.js`, and each native host binary +
   manifest install **root-owned and not user-writable**. A user-writable privileged-JS config or
   native host is a local privilege-escalation hole and is a release blocker. Installed by
   `install-config.sh` (mode 0644) and verified on the staged package by
   `verify-release-permissions.sh` (root-owned, not group/other-writable; `--require-root` on
   `install-config.sh` does the same post-install check).
7. **AI off by default.** No provider, network calls, or telemetry until explicit opt-in; read-only by
   default; per-action confirmation for any action; model output never auto-executed.
8. **Destructive tests are sandboxed.** Lockout/erasure/mount tests run only in disposable
   VMs/containers against throwaway profiles.

## Supply chain & key management

- **Dependencies pinned**; lockfiles committed; a **CycloneDX SBOM** is generated per release
  (`ci/sbom.sh`).
- **Reproducible builds**: containerized, pinned toolchains, `SOURCE_DATE_EPOCH`; published hashes; a
  rebuild-and-diff gate (`ci/repro-diff.sh`, `tests/repro/`) lets a distrustful user verify the binary
  matches the source.
- **Signing keys** live only in HSM / hardware tokens / platform signing services — **never** in the
  repository or CI plaintext. `build/scripts/sign.sh` refuses to run without a key handle and never
  embeds or generates keys. Rotation is documented and keys are per-purpose (GPG repo signing,
  Authenticode, Apple Developer ID).
- **CVE tracking**: `ci/mfsa-track.sh` maps Mozilla Foundation Security Advisories to the pinned
  Firefox version so maintainers always know the coverage gap; SLA target ~1–2 days behind upstream.

## Audit

An external security audit is required before a stable release (Build-Plan §11/§12 Phase 6). The native
host (key handling + untrusted IPC parsing) and the permissions/installer model are the priority audit
surfaces.
