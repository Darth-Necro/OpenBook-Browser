# OpenBook Browser Threat Model

This document covers the build/source-integrity pipeline (Phase 0) and the runtime features added in
Phases 1–6. It is kept consistent with `docs/OpenBook-Browser-Build-Plan.md` (§5.3, §6, §7, §11) and
`CLAUDE.md`. Record changes that affect guarantees in `docs/DECISIONS.md`.

## Assets

- The verified upstream Firefox source tarball and the ordered OpenBook patch series.
- Build scripts, mozconfigs, CI workflows, release artifacts, and signing inputs.
- Privileged runtime files: AutoConfig (`openbook.cfg`, `defaults/pref/*.js`), enterprise
  `policies.json`, the native messaging host binaries and their manifests.
- User data at rest in the profile (cookies, history, credentials, tokens) and the vault keys.
- User network metadata (real IP, DNS queries) when a proxy/tunnel is in use.

## Invariants carried into all phases

- No unsolicited telemetry or first-run network egress.
- Source tarballs verified by hash **and** detached signature before patching; patch application
  fails hard on conflict.
- Privileged config and native-host files are root-owned and not user-writable in release packages.
- Proxy/VPN behavior fails closed. Erasure is cryptographic (key invalidation), not deletion.
- Lockout counters are hardware-enforced where available; the no-hardware fallback is labeled weaker.
- AI is off by default; page content is untrusted; action-taking requires per-action confirmation.
- Destructive lockout / encryption / mount work runs only in disposable harnesses.

## Phase 0 — Build & source integrity

| Adversary | Capability | Mitigation |
|---|---|---|
| Network attacker | Modifies downloads in transit | HTTPS + Mozilla SHA256 manifest and detached-signature verification (`gpgv`). |
| Malicious mirror / stale cache | Serves the wrong source tarball | Exact version pin (145.0.2) + SHA256 entry matching the requested source path. |
| Patch drift | Causes silent partial patching | `apply-patches.sh` exits on the first failed patch. |
| CI misconfiguration | Skips verification / builds wrong target | Matrix workflow runs static checks on every push; full builds are an explicit manual gate. |
| Supply-chain (deps) | Compromised crate/npm package | Pinned dependencies, lockfiles, per-release SBOM (CycloneDX). |

## Phase 1 — Privacy hardening (settings layer)

| Adversary | Capability | Mitigation |
|---|---|---|
| Default telemetry / phone-home | Browser emits data without consent | Telemetry/Normandy/data-reporting prefs `lockPref`-off in `openbook.cfg`, duplicated in `policies.json` (defense in depth), and proven by the CI first-run egress test. |
| Fingerprinter | Correlates users across sites | `resistFingerprinting` (unlocked default, ADR-0014), `fingerprintingProtection`, tracking protection, cookie partitioning. |
| Local privilege escalation | Writes privileged JS into a user-writable install dir | **Permissions invariant (§11):** `openbook.cfg` and `defaults/pref/*.js` ship root-owned, not user-writable. Treated as a release blocker. |
| Pref/policy drift | New upstream pref re-opens a leak | Privacy-regression suite asserts the hardened values; rebased and re-run on every upstream stable. |

## Phase 2 — Cryptographic lockout (data at rest)

This feature addresses a **lost or stolen device / unauthorized physical access**. STRIDE-style summary
(Build-Plan §5.3):

| Adversary | Capability | Result under this design |
|---|---|---|
| Casual finder | Pokes at a running/locked app | Defeated by lock + counter. |
| Offline disk-imaging adversary | Images disk, attacks offline | Defeated: ciphertext + hardware-bound key make offline exhaustive search infeasible; the app counter is irrelevant because crypto-erasure already protects the data. With no hardware, the guarantee degrades to Argon2id over a forced strong passphrase and is labeled weaker. |
| Counter-reset attacker | Power-cycles to reset attempt count | Defeated: counter is monotonic, incremented before each attempt and persisted; resets only on success. |
| "Erase = delete" assumption | Recovers deleted/overwritten data | Defeated: erasure invalidates the hardware-sealed key first (instantaneous crypto-erasure); SSD overwrite is explicitly not relied upon (NIST SP 800-88). |
| Compelled disclosure | Forces the user to unlock | **Out of scope** — documented honestly; no software defeats a user being compelled to enter their secret. |
| Cold-boot / memory scrape | Reads RAM for keys | Mitigated by short lock timeout + key zeroization (`zeroize`); residual risk noted. |
| Malformed-IPC attacker | Sends hostile frames to the native host | Length-capped (1 MiB) framing; fuzzed JSON parser; never panics; returns `invalid-request`. |

Guardrail: all erasure/mount/lockout testing runs only against disposable VMs/containers and throwaway
profiles — never the developer host (Build-Plan §5.4).

## Phase 3 — Proxy/VPN leak protection

The browser owns per-profile proxying + leak prevention, not a system tunnel (ADR-0010). Without **all
four** controls it is theater:

| Vector | Adversary capability | Mitigation |
|---|---|---|
| WebRTC | ICE candidates expose real IP | `peerConnectionEnabled` / `webRTCIPHandlingPolicy` forced to proxy-only or disabled. |
| DNS | OS resolver leaks queries around the proxy | SOCKS with `proxyDNS`/`socks_remote_dns`; DoH (`trr.mode`). |
| IPv6 | v4-only tunnel leaks via v6 | Disable v6 when the tunnel is v4-only; surface a warning. |
| Fail-open | Tunnel drops, traffic goes direct | Blocking `webRequest` kill-switch cancels all requests when the health-check fails or the kill-switch is engaged; never silent direct fallback. |

The leak suite (`tests/leak/`) must be green on all four vectors including on tunnel failure.

## Phase 4 — AI sidebar

| Adversary | Capability | Mitigation |
|---|---|---|
| Unwanted data exposure | Browsing context sent to a third party | Off by default; local-first (Ollama on localhost); no provider/network/host-permission until explicit opt-in; BYO-key path surfaces the egress implication. |
| Prompt injection | Malicious page content steers the assistant | Page content is wrapped as untrusted data, not instructions; assistant is read-only by default; **every action requires explicit per-action confirmation**; model output is never auto-executed. |
| Telemetry creep | Assistant phones home | Zero telemetry; verified by the egress test with the feature enabled. |

Prompt injection is a live, unsolved attack class — the mitigation is containment (read-only +
confirmation), not a claim of prevention.

## Phase 5/6 — Release & supply chain

| Adversary | Capability | Mitigation |
|---|---|---|
| Tampered release artifact | Ships a backdoored binary | Reproducible builds (pinned toolchains, `SOURCE_DATE_EPOCH`), published hashes, repro-diff; per-OS signing (GPG/Authenticode/Developer ID + notarization). |
| Signing-key theft | Signs malicious builds | Keys only in HSM/hardware tokens, never in repo/CI plaintext; documented rotation. |
| Stale CVE exposure | Ships known-vulnerable Gecko | MFSA tracking maps advisories to the pinned version; ~1–2 day SLA behind upstream stable. |

## Out of scope

- Compelled disclosure / rubber-hose attacks (documented, not defeated).
- A fully malicious OS or compromised hardware root of trust.
- Defeating a global passive network adversary's traffic analysis (mitigated, not eliminated).
