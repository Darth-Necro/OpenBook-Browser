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

## ADR-0015 — Phase 1 preference hardening mechanism (carried from PR #2)

- **Date:** 2026-06-08
- **Decision:** Use Firefox AutoConfig (`defaults/pref/autoconfig.js` + `openbook.cfg`) for locked preferences, supplemented by Mozilla enterprise policies (`distribution/policies.json`).
- **Options considered:** AutoConfig only; enterprise policies only; `user.js` (erased on profile reset); compiled-in pref defaults.
- **Rationale:** AutoConfig's `lockPref()` prevents user override without UI changes; enterprise policies add a JSON audit trail and OS-management integration. Both are supported in unmodified Firefox stable. Compiled-in defaults require source patches and raise maintenance burden; a `user.js` offers no protection against modification.
- **Constraint:** `openbook.cfg` is privileged and must be root-owned, not user-writable, in releases (invariant 6).

## ADR-0016 — Phase 1 branding mechanism (carried from PR #2)

- **Date:** 2026-06-08
- **Decision:** Add `browser/branding/openbook/` via an ordered patch and select it with `--with-branding=browser/branding/openbook` in every per-platform mozconfig; binary brand assets live in `branding/` and are staged into the source tree by `build.sh` before the build.
- **Options considered:** Modify `browser/branding/official/` in place; add a separate brand directory; AutoConfig display strings only.
- **Rationale:** A separate branding directory isolates OpenBook identity from upstream and minimises rebase churn; mozconfig selection keeps the Firefox build system authoritative.
- **Note:** Supersedes the earlier `moz.configure`-default branding patch (removed in the PR #2 ↔ #3 merge) in favour of explicit mozconfig selection plus `build.sh` asset staging.

## ADR-0017 — Release versioning and tag-driven draft releases

- **Date:** 2026-06-11
- **Decision:** Version releases as `<upstream-firefox-version>-<openbook-build>`
  (e.g. `145.0.2-1`), single source of truth in the root `VERSION` file, release
  tags `v<VERSION>`. A `v*` tag triggers `.github/workflows/release.yml`, which
  re-runs every offline gate and assembles the artifacts hosted CI can honestly
  produce — deterministic extension XPIs, linux-x64 native-host binaries +
  manifests, the settings overlay tarball (`package-components.sh`), a strict
  CycloneDX SBOM, and `SHA256SUMS` — into a **draft** GitHub release. Signing
  stays exclusively on maintainer hardware (`sign.sh`; keys in HSM/tokens, §11),
  and full browser packages come from the per-OS build hosts; the draft is
  published only after `docs/RELEASE-CHECKLIST.md` is fully checked.
  `tests/release/test_release_layer.py` gates VERSION/pin/changelog/workflow
  consistency in CI.
- **Options considered:** Independent semver for the fork; upstream-version-suffix
  scheme (LibreWolf-style); fully automated publish on tag; signing in CI with
  repository secrets.
- **Rationale:** The fork tracks upstream stable, so the upstream version is the
  meaningful identity and the suffix the OpenBook iteration — matching the peer
  forks users already understand. Draft-not-publish keeps the human checklist and
  hardware-held signing as the final gate; publishing unsigned artifacts or
  holding signing keys as CI secrets would each violate §11.

## ADR-0018 — In-app updater off; remaining background egress disabled or documented

- **Date:** 2026-06-11
- **Decision:** Disable the in-app updater entirely (`DisableAppUpdate: true`,
  locked `app.update.*` prefs, and `--disable-updater`/`--disable-crashreporter`
  in the release mozconfigs; `--disable-default-browser-agent` on Windows).
  Updates ship via the signed package channels chosen in ADR-0013 on the
  MFSA-tracked 1–2 day SLA. Additionally: the system-add-on update pipeline is
  locked off (OpenBook's bundled extensions update with the browser package);
  the remaining fresh-profile endpoints are recorded as **documented
  exceptions** in `openbook.cfg` §12 (Remote Settings/OneCRL, Safe Browsing
  list updates, GMP fetches, AMO update checks for user-installed extensions,
  and the user-configured DoH resolver), which the first-run egress test
  enforces as the complete allowlist. Bundled extensions install via Mozilla's
  `distribution/extensions/` mechanism instead of a speculative source patch.
- **Options considered:** Keep the updater pointed at Mozilla (rejected:
  unsolicited egress on a timer, and it would offer stock Firefox MARs to an
  OpenBook install); stand up AUS/Balrog now (rejected: ADR-0013 already chose
  package channels first); disable Remote Settings/Safe Browsing/GMP too
  (rejected: each removes a security or core-function service — revoked-CA
  protection, malware-list updates, codec/DRM — which is a worse user outcome
  than their documented, history-free connections).
- **Rationale:** "Zero telemetry / no unsolicited egress" is only auditable if
  every endpoint is either off or explicitly justified; this ADR makes the
  config layer match the distribution strategy and gives the egress test a
  precise contract. Supersedes the earlier "keep updates on" posture in the
  cfg/policy tests, whose security goal (patches keep flowing) is met by the
  package channels instead.
