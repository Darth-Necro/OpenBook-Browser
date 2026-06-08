# OpenBook Browser Decision Log

## ADR-0001 — Firefox source pin for Phase 0

- **Date:** 2026-06-08
- **Decision:** Pin Phase 0 to Firefox `145.0.2` source tarball from Mozilla release infrastructure.
- **Options considered:** Track `latest/`; pin a numbered stable release; use ESR.
- **Rationale:** A numbered stable release gives reproducible source URLs and immutable verification inputs. `latest/` is convenient but not deterministic. ESR is valuable for long-lived maintenance but the kickoff asks for Firefox stable, and Phase 0 must first prove the unmodified stable build pipeline.
- **Follow-up:** Revisit ESR versus rapid stable as an explicit release-channel decision before Phase 1.

## ADR-0002 — Patch application mechanism

- **Date:** 2026-06-08
- **Decision:** Use an ordered git patch series applied with `git am --3way` when the extracted upstream source is a git worktree, otherwise fall back to `patch -p1`.
- **Options considered:** `git format-patch` series; quilt; ad-hoc file copies.
- **Rationale:** Ordered git patches are deterministic, auditable, and rebaseable across upstream Firefox stable releases. Quilt remains possible later, but git patches are simpler for initial CI.

## ADR-0003 — CI provider skeleton

- **Date:** 2026-06-08
- **Decision:** Start with GitHub Actions workflow files and keep full Firefox builds manually gated.
- **Options considered:** GitHub Actions; GitLab CI; self-hosted-only build scripts.
- **Rationale:** GitHub Actions offers a straightforward Linux, Windows, and macOS matrix for proving orchestration. Full Firefox builds are resource-intensive, so CI exposes a manual gate while local/static checks run on every push.

## ADR-0004 — Phase 0 governance/funding posture

- **Date:** 2026-06-08
- **Decision:** Document a nonprofit/donations/grants posture as the default sustainability model until a formal governance decision is made.
- **Options considered:** Nonprofit/donations/grants; parent commercial sponsor; ad/search/telemetry monetization.
- **Rationale:** The security invariants forbid telemetry and unwanted data exposure. Ad/search/telemetry-driven funding conflicts with the project mission. A parent sponsor may be viable later, but the initial public posture should preserve independence and user trust.

## ADR-0005 — OpenBook own-code license

- **Date:** 2026-06-08
- **Decision:** License OpenBook's own code under **MPL-2.0**.
- **Options considered:** MPL-2.0; GPL-3.0; Apache-2.0/MIT.
- **Rationale:** Upstream Firefox is MPL-2.0. Matching it removes friction when code moves between the patch layer and upstream, keeps file-level license headers consistent, and matches peer forks (LibreWolf). Copyleft at the file level fits a privacy project without the broader obligations of GPL-3.0 on the native host and tooling.

## ADR-0006 — Profile-at-rest encryption mechanism

- **Date:** 2026-06-08
- **Decision:** Ship **Option 1 (userspace AEAD-encrypted container)** for v1, with **Option 2 (OS-native FDE primitives: LUKS/fscrypt, encrypted APFS, BitLocker-backed VHDX)** as a per-platform hardening upgrade. Reject Option 3 (per-file AEAD patched into Gecko I/O) for v1; treat Option 4 (FDE-delegation + small vault) as a documented fallback only.
- **Options considered:** The four options in Build-Plan §5.2.
- **Rationale:** One portable codebase that the native host can encrypt/decrypt deterministically gives the cleanest cross-platform v1 and is testable in a disposable harness. OS-native FDE is better-audited but triples the code paths and needs elevated privileges, so it is a follow-up. Deep Gecko I/O patching is high-maintenance and brittle across upstream releases.
- **Follow-up:** Evaluate gocryptfs/FUSE vs a custom AEAD container and the OS-FDE upgrade per platform before the Phase 2 gate.

## ADR-0007 — Cryptographic-lockout key design

- **Date:** 2026-06-08
- **Decision:** `KEK = HKDF-SHA256( Argon2id(secret, salt, RFC-9106 params) XOR hardware_secret )`; KEK wraps a random 256-bit Master Key (AEAD); the MK encrypts the profile container. Attempt counter is monotonic, incremented **before** each attempt and persisted immediately, reset **only** on success. Cryptographic erasure **invalidates the hardware-sealed secret first** (making the wrapped MK permanently undecryptable), then best-effort deletes the container, then zeroizes memory. Escalating delays precede the irreversible step.
- **Options considered:** PIN-only KDF (rejected — offline brute-force of small keyspace); KDF without hardware binding (rejected — counter is bypassable on a disk image); deletion/overwrite as "erasure" (rejected — unreliable on SSDs per NIST SP 800-88).
- **Rationale:** Binding to a non-extractable hardware secret is what makes the offline attack and the bypassable-counter failure modes actually defeated (Build-Plan §5). Argon2id raises per-guess cost but cannot save a weak PIN alone.
- **Follow-up:** TPM 2.0 (`tss-esapi`, NV-index counter + sealed blob) and Secure Enclave providers are feature-gated and validated on real hardware in the Phase 2 harness.

## ADR-0008 — No-hardware fallback posture

- **Date:** 2026-06-08
- **Decision:** With no TPM 2.0 / Secure Enclave, fall back to Argon2id over a **forced strong passphrase** (block all-digit and length < 12), and **label the guarantee as weaker** in the UI and host status (`hardware: "software"`). The attempt limit is advisory against disk imaging in this mode.
- **Options considered:** Refuse to run without hardware; silently degrade; degrade with explicit labeling.
- **Rationale:** Invariant 4 requires hardware enforcement where available and an honest, labeled weaker guarantee otherwise. Refusing entirely would exclude users on hardware without a usable TPM; silent degradation would mislead.

## ADR-0009 — Native messaging transport

- **Date:** 2026-06-08
- **Decision:** Firefox native messaging framing: a 4-byte message length in **native byte order** followed by UTF-8 JSON, with a **1 MiB** per-message cap; malformed frames return `invalid-request` and never panic. Protocol verbs: `status`, `setup`, `unlock`, `lock`, `erase` (vault host); `status`, `verify` (vpn-helper).
- **Options considered:** Native messaging stdio (chosen); a local socket/HTTP daemon (rejected — larger attack surface, lifecycle/permission complexity).
- **Rationale:** Matches the Firefox-managed model where the browser validates the host manifest but does not manage the host — which drives the root-owned/not-user-writable permissions invariant (§11). The JSON parser is fuzzed.

## ADR-0010 — Proxy/VPN model

- **Date:** 2026-06-08
- **Decision:** Support **Option 1 (OS-level VPN; browser verifies exit IP)** as the real-tunnel model plus **Option 2 (per-profile SOCKS5/HTTP proxy via `browser.proxy`)** for in-browser convenience, gated by **all four leak controls** (WebRTC, DNS, IPv6, fail-closed). Defer `vpn-helper` to a verification-only scaffold. **Reject bundled userspace WireGuard (Option 3) for v1.**
- **Options considered:** The four options in Build-Plan §6.
- **Rationale:** A browser is the wrong layer to own a system tunnel (kernel networking). Per-profile proxying + leak prevention is what the browser can do correctly; shipping a network stack and a privileged `tun` device is disproportionate risk for v1.

## ADR-0011 — AI provider architecture

- **Date:** 2026-06-08
- **Decision:** **Option 3 (pluggable provider abstraction)** shipping **zero providers enabled**, defaulting to **Option 1 (local Ollama/llama.cpp on localhost)** when configured. Off by default (no provider, no network, no host permissions until opt-in). Read-only by default; page content treated as untrusted; every action requires explicit per-action confirmation; model output is never auto-executed.
- **Options considered:** The four options in Build-Plan §7.
- **Rationale:** Resolves the "no unwanted data exposure" vs cloud-AI contradiction by being local-first and opt-in, while the abstraction still permits an informed bring-your-own-key path with a surfaced egress warning.

## ADR-0012 — Extension manifest version and tooling

- **Date:** 2026-06-08
- **Decision:** Bundled extensions use **Manifest V2** with TypeScript and `web-ext`; each sets `browser_specific_settings.gecko.id`.
- **Options considered:** MV2; MV3.
- **Rationale:** Firefox supports MV2 fully, including persistent background pages, blocking `webRequest` (required for the fail-closed kill-switch), and `nativeMessaging`. MV3's blocking-request restrictions would undermine the fail-closed guarantee. Revisit if Firefox deprecates MV2.

## ADR-0013 — Update distribution

- **Date:** 2026-06-08
- **Decision:** Start with **Option 2 (OS package managers + Flatpak/Homebrew/winget/Chocolatey)**; migrate toward **self-hosted signed-MAR (Option 1) / hybrid (Option 3)** if security patches need to ship faster than package channels allow.
- **Options considered:** The four options in Build-Plan §9.
- **Rationale:** Lowest operational burden to start, matches LibreWolf, and avoids standing up Balrog + MAR signing infrastructure before it is warranted. SLA target ~1–2 days behind upstream stable, tracked via MFSA automation.

## ADR-0014 — `resistFingerprinting` ships unlocked

- **Date:** 2026-06-08
- **Decision:** Ship `privacy.resistFingerprinting` enabled via `defaultPref` (not `lockPref`), letting users opt out.
- **Options considered:** Lock it on; ship on but unlocked; leave it off.
- **Rationale:** RFP is a strong anti-fingerprinting control but imposes real UX costs (timezone, letterboxing, canvas prompts). Locking it would trap users who hit breakage; shipping it on-by-default-but-changeable preserves the privacy posture while respecting user control. Telemetry/Normandy/data-reporting prefs remain `lockPref` because there is no legitimate reason to re-enable them.
