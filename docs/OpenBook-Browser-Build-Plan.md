# OpenBook Browser — Complete Build Plan (Firefox/Gecko Fork)

**Approach:** Option A — fork Firefox stable in the LibreWolf / Mullvad Browser / Tor Browser model.
**Targets:** Linux, Windows, macOS.
**Goal:** Free, open-source, privacy-hardened browser; zero telemetry; opt-in AI assistant (off by default); user-controlled proxy/VPN integration with leak protection; hardware-backed data-at-rest protection using cryptographic erasure — i.e. lost/stolen-device protection in the model of iOS Data Protection and OS full-disk encryption.

> This document is the authoritative reference. The companion `CLAUDE.md` is the operational guardrail file for the repo root, and the Claude Code kickoff prompt drives execution. All three must stay consistent; record deviations in `docs/DECISIONS.md`.

---

## 0. Scope reality (read before anything)

A Firefox fork is not "writing a browser." Upstream Firefox (`mozilla-central` / the released `firefox-<ver>.source.tar.xz`) is a ~20M+ line tree. A **full** build requires a 64-bit host, **≥30 GB free disk (≥40 GB on Windows), 8 GB+ RAM, Python 3.9+**, the `mach` build system, and hours of compilation. The LibreWolf/Mullvad model is therefore a **build-and-patch-and-package repository** layered on top of an upstream source tarball — patches, theming, build scripts, and a settings layer (`.cfg` autoconfig + `policies.json`).

What is tractable to build to completion in code:

- The patch / branding / settings layer over upstream.
- The OpenBook-specific features: bundled WebExtensions (UI) + a native messaging host (crypto / TPM / cryptographic erasure).
- The CI/CD that fetches, verifies, patches, builds, signs, packages, and releases.
- Reproducible-build tooling, tests, threat model, and docs.

What is **orchestrated, not run inline:** the engine build itself — invoked by scripts on the build host / CI runners, not in an interactive session. For frontend-only iteration (autoconfig, policies, branding, extensions) use **artifact builds** (`ac_add_options --enable-artifact-builds`), which download prebuilt internals and skip the multi-hour C++ compile. Only patches that touch C++ backend code require full builds.

---

## 1. Architecture

The fork is four cleanly separated layers. Keeping them separate is what makes the project maintainable across upstream Firefox releases.

```
            ┌─────────────────────────────────────────────────────────┐
            │  Upstream Firefox stable source tarball (pinned + verified)│
            └─────────────────────────────────────────────────────────┘
                         │ apply (ordered patch series)
        ┌────────────────┼───────────────────────────────────────────┐
        ▼                ▼                                             ▼
  (1) PATCHES       (2) SETTINGS LAYER                          (3) BRANDING
  branding/privacy/ autoconfig openbook.cfg (privileged JS:     name, icons,
  features patches  pref/defaultPref/lockPref) + policies.json   strings
        │                │                                             │
        └────────────────┴──────────────────────────┬──────────────────┘
                                                     ▼
                                          BUILD (mach + mozconfig)
                                                     │
                              ┌──────────────────────┼──────────────────────┐
                              ▼                       ▼                      ▼
                     (4a) BUNDLED EXTENSIONS   (4b) NATIVE HOST        PACKAGE + SIGN
                     vault-ui (lockout UI),    vault-host              deb/rpm/flatpak/
                     proxy-manager,            (Rust: Argon2id, TPM/   appimage, dmg/pkg,
                     ai-sidebar (TypeScript)   Secure Enclave,         exe/msi
                                               key erasure)
```

**Why this split:**

- **Preference hardening + locking → AutoConfig.** A `defaults/pref/autoconfig.js` containing `pref("general.config.filename","openbook.cfg")` plus `openbook.cfg` at the install root. AutoConfig functions: `pref()` (sets user value), `defaultPref()` (sets default), `lockPref()` (sets default and locks — most common). The `.cfg` is **privileged JavaScript**, so a syntax error throws a startup error; values must be real JS literals (`false`, not `"false"`). The first line of the `.cfg` is always ignored by the parser and must be a comment.
- **Policy-level controls → `distribution/policies.json`** (Mozilla policy engine). Use policies for what they cover (search engines, extension install, feature disabling), AutoConfig for everything else. Reference `arkenfox/user.js` as a hardening source but ship OpenBook's own values.
- **UI features → WebExtensions** bundled in the build.
- **Crypto / TPM / Secure Enclave / profile-key erasure → a native messaging host.** WebExtensions cannot touch hardware key stores or perform cryptographic erasure; a native host can. The host speaks length-prefixed JSON over stdio; the extension holds the `nativeMessaging` permission; the host manifest's `allowed_extensions` lists the extension ID. **Critically: the browser reads/validates the host manifest but does not install or manage it — its security model is that of a native application,** which drives the permissions invariant in §11.

---

## 2. Repository layout

```
openbook/
├── build/
│   ├── mozconfig/                 # mozconfig.linux-x64, .win-x64, .macos-universal, .artifact
│   ├── scripts/
│   │   ├── fetch-verify-upstream.sh   # pin version, download tarball, verify hash/signature
│   │   ├── apply-patches.sh           # ordered series; fail hard on conflict
│   │   ├── build.sh                   # mach build wrapper, per-target
│   │   ├── package.sh                 # per-OS packaging
│   │   └── sign.sh                    # per-OS signing
│   └── docker/                    # reproducible build images (pinned toolchains)
├── patches/
│   ├── branding/                  # de-Firefox, OpenBook identity
│   ├── privacy/                   # hardening that can't be done via prefs/policy
│   └── features/                  # hooks the bundled extensions/native host need
├── branding/                      # icons, names, strings (replaces browser/branding)
├── config/
│   ├── autoconfig/                # openbook.cfg, autoconfig.js
│   ├── policies/                  # policies.json
│   └── distribution/              # distribution/ dir + prefs
├── extensions/
│   ├── vault-ui/                  # lock screen, setup, attempt UI (TypeScript)
│   ├── proxy-manager/             # proxy switching + fail-closed UI (TypeScript)
│   └── ai-sidebar/                # opt-in assistant (TypeScript)
├── native/
│   ├── vault-host/                # Rust: Argon2id, TPM2/Secure Enclave, container, key erasure
│   └── vpn-helper/                # optional Rust helper (later phase)
├── ci/                            # pipelines, matrix, signing, SBOM, repro-diff
├── tests/
│   ├── native/                    # Rust unit/integration/property/fuzz
│   ├── extensions/                # jest + web-ext/Marionette/Playwright-Firefox
│   ├── privacy-regression/        # telemetry-off, RFP-on, leak assertions
│   ├── leak/                      # WebRTC/DNS/IPv6/fail-open harness
│   └── repro/                     # reproducible-build diff
└── docs/
    ├── OpenBook-Browser-Build-Plan.md
    ├── THREAT-MODEL.md
    ├── BUILD.md
    ├── SECURITY.md
    └── DECISIONS.md               # ADR log
```

---

## 3. Tech stack and rationale

| Layer | Choice | Why |
|---|---|---|
| Native host / helpers | Rust | Memory safety on the component that handles keys and parses untrusted IPC; mature crates (`argon2`, `tss-esapi` for TPM 2.0, platform bindings for Secure Enclave/Keychain/DPAPI). |
| Extensions | TypeScript + `web-ext` | Type safety, standard Firefox extension tooling, no bundled remote SDKs. |
| Build orchestration | bash + Python | Matches `mach` ecosystem; portable. |
| Patch management | `git format-patch` series or `quilt` | Deterministic, ordered, rebaseable onto each Firefox release. |
| Reproducibility | Docker, pinned toolchains, `SOURCE_DATE_EPOCH` | Lets users verify the binary matches the source (the trust mechanism Tor/LibreWolf use). |
| CI | GitHub Actions or GitLab CI, matrix | linux x64/arm64, windows x64, macOS universal. |

---

## 4. Settings and hardening layer

Two mechanisms, used together:

**AutoConfig (`config/autoconfig/`)**

- `autoconfig.js` → `defaults/pref/` with:
  ```js
  pref("general.config.filename", "openbook.cfg");
  pref("general.config.obscure_value", 0);
  ```
- `openbook.cfg` → install root. Privileged JS using `lockPref`/`defaultPref`/`pref`. Baseline (non-exhaustive):
  - Telemetry/coalition off: `toolkit.telemetry.enabled`, `toolkit.telemetry.server`, `datareporting.*`, `app.shield.optoutstudies.enabled`, `browser.discovery.enabled`, `browser.newtabpage.activity-stream.feeds.telemetry`.
  - Anti-fingerprinting: `privacy.resistFingerprinting`, `privacy.fingerprintingProtection`.
  - DNS/connection: DoH default (`network.trr.mode`), `network.proxy.socks_remote_dns` when SOCKS is used.
  - WebRTC: handled by the proxy-manager (§6), not a blanket disable unless the user opts in.
  - Disable Pocket, sponsored tiles, default-browser nagging, Normandy, captive-portal phone-home.

**Enterprise policy (`config/policies/policies.json`)**

- Use the Mozilla policy engine for: default search engine (privacy-respecting), disabling telemetry/studies at policy level (defense in depth), controlling extension install, disabling features not removable by pref. Validate against `github.com/mozilla/policy-templates`.

**Permissions invariant (see §11):** both the `.cfg` and the `defaults/pref/*.js` files are privileged. If the install directory is user-writable, an adversary can inject privileged JS = local code execution. Installers must place these root-owned, non-user-writable.

---

## 5. Feature: Cryptographic Lockout (data-at-rest protection)

This is the flagship data-protection feature. It is the same class of mechanism as iOS Data Protection (failed-passcode erasure) and OS full-disk encryption: the threat it addresses is **a lost or stolen device, or unauthorized physical access to the machine.**

**User-facing behavior:** the browser profile is held in an encrypted vault, unlocked by a PIN/passphrase. After N failed unlock attempts (default 6), the vault key is cryptographically erased, rendering the profile data unrecoverable.

**The two failure modes this design must defeat (any serious adversary will probe both immediately):**

1. **App-enforced attempt counters are bypassable.** Anyone who images the disk ignores the counter and attacks the data offline. The counter is a real control **only** if hardware enforces it.
2. **Deletion is recoverable, and SSD overwrite is unreliable** (wear-leveling, over-provisioning). The only reliable form of erasure is **cryptographic erasure**: encrypt the profile at rest; "erase" = invalidate the key.

Plus the **PIN-strength problem:** a 4–6 digit PIN is a 10⁴–10⁶ keyspace. If the data key derives from the PIN alone and the adversary holds the ciphertext, they can run an offline exhaustive search regardless of the counter. Argon2id raises per-guess cost; it does not save a 4-digit PIN. The real fix is binding the key to hardware so a non-extractable hardware secret is required, and the hardware enforces rate-limiting and key invalidation.

### 5.1 Cryptographic design

```
Master Key (MK)  : 256-bit random, generated once. Encrypts the profile container.
KEK              : Key-Encryption-Key wrapping MK.
KEK = HKDF( Argon2id(secret, salt, high-cost params)  ⊕  hardware_secret )
hardware_secret  : sealed in TPM 2.0 (sealed blob / NV index with PCR + auth policy) on
                   Linux/Windows; non-extractable Secure Enclave key on macOS. Cannot be
                   read out; can only be *used* on the device, gated by the OS/hardware.
Attempt counter  : monotonic, stored in TPM NV index (TPM enforces dictionary-attack
                   lockout with escalating delays) or Secure-Enclave-guarded storage.
                   Increment BEFORE each attempt; reset ONLY on success → power-cycling
                   cannot reset it.
```

**Unlock:** user enters secret → host increments counter → derives KEK → unwraps MK → unlocks the encrypted profile container → on success resets counter.

**Cryptographic erasure (Nth failure or explicit user trigger):**

1. Invalidate the hardware-sealed key material first — **instantaneous crypto-erasure**; the wrapped MK is now permanently undecryptable.
2. Best-effort delete the ciphertext container.
3. Zeroize any in-memory key material.

**Escalating delays** before the irreversible step (Secure-Enclave-style increasing timeouts) reduce accidental erasure by a legitimate user mistyping the PIN.

**No-recovery acknowledgement** at setup: the whole point is irrecoverability; make the user explicitly accept it.

### 5.2 Profile-at-rest encryption — four options

Firefox writes its profile in cleartext to disk; crypto-erasure requires the profile to be encrypted at rest. Choose the container mechanism:

1. **Userspace cross-platform encrypted FS** (gocryptfs-style). *Pro:* one codebase, portable, native host mounts on unlock / unmounts on lock. *Con:* you ship and maintain an FS layer.
2. **OS-native FDE primitives per platform** (LUKS-on-loopback / fscrypt on Linux; encrypted APFS volume or DMG on macOS; BitLocker-backed VHDX or userspace FS on Windows). *Pro:* audited OS crypto. *Con:* three code paths and privilege requirements.
3. **Application-level per-file AEAD** patched into Gecko's profile I/O. *Pro:* tightest integration. *Con:* very hard, deep engine patching, high maintenance. **Reject for v1.**
4. **FDE-delegation + small crypto-erased vault** — rely on the user's full-disk encryption and only crypto-erase a vault of the most sensitive data (cookies, history, saved credentials, tokens). *Pro:* lightest. *Con:* partial guarantee; the rest of the profile is unprotected if the disk isn't FDE'd.

**Recommendation:** **Option 1** for portability in v1, with **Option 2** as a hardening upgrade where the OS primitive is available. Record the choice in `DECISIONS.md`.

### 5.3 Threat model (STRIDE-style summary)

| Adversary | Capability | Result under this design |
|---|---|---|
| Casual finder of a lost/stolen device | Pokes at a running/locked app | Defeated by lock + counter. |
| Offline disk-imaging adversary | Images the disk, attacks offline | Defeated: ciphertext + hardware-bound key make offline exhaustive search infeasible; the app counter is irrelevant because crypto-erasure already protects the data. |
| Network adversary | Intercepts traffic | Out of scope for this feature (handled by §6 + TLS). |
| Compelled disclosure | Forces the user to unlock | **Out of scope.** Document honestly; no software defeats a user being compelled to enter their secret. |
| Cold-boot / memory scrape | Reads RAM for keys | Mitigated by short lock timeout + key zeroization; note residual risk. |

**Honest fallback:** with no TPM/Secure Enclave, drop to Argon2id over a **forced strong passphrase** (block 4-digit PINs) and state in the UI that the guarantee is weaker and the attempt limit is advisory against disk imaging.

### 5.4 Components and tests

- `native/vault-host` (Rust): KDF, hardware sealing, counter, container mount/lock, key erasure. Unit + property tests; **KDF test vectors**; **fuzz the native-messaging JSON parser**; integration tests in the VM harness only.
- `extensions/vault-ui` (TS): setup wizard, lock screen, attempt feedback, escalating-delay UI, no-recovery consent.
- **Destructive-work guardrail:** all erasure/mount/lockout testing runs against disposable VMs/containers and throwaway profiles — never the developer host.

---

## 6. Feature: Proxy / VPN manager (user-controlled)

**Core truth:** a browser is the wrong layer to run a full system VPN tunnel — that's kernel/OS networking. The browser owns **per-profile proxying + leak prevention.**

**Options (the four-way decision):**

1. **OS-level VPN, browser stays out** (user runs WireGuard/OpenVPN; browser optionally verifies exit IP). *Supported real-tunnel model.*
2. **Per-profile SOCKS5/HTTP proxy** via `browser.proxy` (point at the user's own endpoint). Browser-native, easy — **only safe with strict fail-closed + leak controls.**
3. **Bundled userspace WireGuard** managed by the browser UI (`boringtun`/`wireguard-go`). "VPN in the browser" UX, but you ship a network stack + handle the privileged `tun` device. **Avoid for v1.**
4. **Proxy-in-a-tab / compartmentalized routing** (Brave "Tor in a tab" generalized). Good for compartmentalization; hard to make leak-proof.

**Recommendation:** **Option 1** as the supported tunnel + **Option 2** for in-browser convenience, with all four leak controls enforced and `vpn-helper` deferred.

**The four mandatory leak controls (without all four it is theater):**

1. **WebRTC** — disable (`media.peerconnection.enabled`) or force through the proxy; never let ICE candidates expose the real IP.
2. **DNS** — force resolution through the proxy/tunnel (`network.proxy.socks_remote_dns=true`) and/or DoH (`network.trr.mode`); never let the OS resolver bypass it.
3. **IPv6** — ensure the tunnel covers v6, or disable v6 if the tunnel is v4-only.
4. **Fail-closed** — if the proxy/tunnel drops, block traffic; never silently go direct.

**Components and tests:** `proxy-manager` extension (proxy config + fail-closed enforcement UI) + policy/pref backing. `tests/leak/` runs the browser against a controlled proxy and **asserts no traffic escapes** on any of the four vectors, including on tunnel failure.

---

## 7. Feature: AI sidebar (opt-in, off by default)

**Hard contradiction to resolve:** "no unwanted data exposure" + cloud AI = sending browsing context to a third party. Resolve by **local-first.**

**Provider options (the four-way decision):**

1. **Local model via Ollama / llama.cpp on localhost** — nothing leaves the machine. Best fit.
2. **Bring-your-own-API-key** — flexible; data goes to that provider under their terms; ship no key, surface the egress implication in-UI.
3. **Pluggable provider abstraction** — interface for local + remote; ship zero providers enabled. *Architecturally correct; subsumes 1 and 2.*
4. **No built-in AI; separately-installed sandboxed extension** — keeps the trusted core minimal.

**Recommendation:** **Option 3**, defaulting to **Option 1** (local) when configured.

**Non-negotiables:**

- OFF by default: no provider, no network calls, no telemetry until the user opts in (feature flag + no host permissions granted).
- **Page content is untrusted input — prompt injection is a live, unsolved attack class.** Assistant is **read-only by default;** any "take action" capability requires explicit per-action confirmation.
- Sandbox the integration; treat model output as untrusted; no auto-execution.

---

## 8. Build, packaging, signing

**Build:** `mach` + per-platform `mozconfig`. Frontend iteration uses artifact builds; release builds are full, containerized, with pinned toolchains.

**Per-OS packaging + signing:**

- **Linux:** `.tar.xz` + `.deb`/`.rpm` (GPG-signed repos) + Flatpak (own remote or Flathub) + AppImage. Mirrors LibreWolf's distribution surface (deb/rpm/Flathub/AUR/AppImage/Chocolatey/MS Store).
- **Windows:** installer (NSIS or WiX/MSI) + portable zip; **Authenticode** signing (EV cert strongly preferred to avoid SmartScreen friction). Optional winget/Chocolatey/MSIX.
- **macOS:** **universal binary** (x86_64 + arm64); **codesign with a Developer ID, notarize via `notarytool`, staple;** ship `.dmg` + `.pkg`. (Without notarization, Gatekeeper blocks it.)

**Reproducible builds:** containerized, pinned toolchains, `SOURCE_DATE_EPOCH`, documented rebuild procedure, output hashes published. This is the mechanism that lets a distrustful user verify the binary matches the source. v2 target: align with Tor's `rbm`-style fully reproducible pipeline.

---

## 9. CI/CD, patch maintenance, updates

**Pipeline (on upstream Firefox stable tag):** fetch source → **verify hash/signature (fail hard on mismatch)** → apply patch series (fail + alert on conflict) → matrix build → tests → package → sign → publish with checksums + signatures + SBOM.

**Patch maintenance:** rebase the patch series onto each new Firefox stable. CI tests patch application on every upstream release. **Track Mozilla Foundation Security Advisories (MFSA)** so you always know which CVEs your current build does/doesn't contain. Forks like LibreWolf run ~1–2 days behind upstream stable; treat that as the SLA target.

**Update distribution — four options:**

1. **Self-hosted Firefox-native update** (AUS/Balrog) with **signed MAR** files — true in-browser auto-update; you run infra + manage MAR signing keys.
2. **OS package managers + Flatpak/Homebrew/winget/Chocolatey** — leverage existing infra, no custom server; update latency varies by channel. *Matches LibreWolf.*
3. **Hybrid** — package managers for Linux/macOS, self-hosted MAR for Windows portable.
4. **AppImage/portable with built-in updater** (AppImageUpdate/zsync) — simple for portable; weak for system installs.

**Recommendation:** start with **Option 2** (lowest burden), migrate toward **Option 1/3** if security patches need faster push than package channels allow.

---

## 10. Testing strategy

- **Native host (Rust):** unit + integration + property tests; crypto test vectors; fuzz the stdio/JSON parser; cross-platform CI.
- **Extensions (TS):** unit (jest) + integration (web-ext / Marionette / Playwright-Firefox).
- **Privacy regression:** assert telemetry endpoints unreachable/blocked on first run; assert `resistFingerprinting` active; run against fingerprint test pages (e.g., EFF Cover Your Tracks); **a network-egress test that fails on any unexpected outbound connection.**
- **Leak tests:** the §6 harness for WebRTC/DNS/IPv6/fail-open.
- **Reproducible-build diff:** rebuild in a clean container and diff against the released artifact.

---

## 11. Security and governance

- **Permissions invariant:** the autoconfig `.cfg`, `defaults/pref/*.js`, and the native host binary + its manifest must be installed **root-owned, not user-writable.** A user-writable privileged-JS config or native host is a local privilege-escalation hole (this exact misconfiguration is a known real-world finding against Firefox enterprise deployments). Treat any deviation as a release blocker.
- **Vulnerability disclosure policy + bug bounty** — fitting for a privacy/security project; publish `security.txt` and a disclosure process.
- **Signing-key management** — keys in HSM / hardware tokens; documented rotation; never in CI plaintext.
- **Supply chain** — pin all dependencies; verify the upstream Firefox source hash/signature; generate an SBOM per release.
- **Telemetry = none, and proven** — verified by patch + policy + the CI egress test.
- **External audit** before a stable release.
- **Sustainability:** "free forever, no data harvesting" is a funding commitment, not code. Credible peers run on either a nonprofit + donations/grants model (e.g., a 501(c)(3), like the Ladybird Browser Initiative) or a parent commercial product (e.g., Mullvad VPN funding Mullvad Browser). Mozilla's own dependence on a search deal is the cautionary case. Decide the model early.

---

## 12. Phased roadmap

Each phase has deliverables, acceptance criteria (the gate to the next phase), and primary risks.

**Phase 0 — Foundations.**
Deliverables: repo tree; `fetch-verify-upstream.sh` (pin + verify); `apply-patches.sh`; per-platform mozconfigs; CI skeleton; `THREAT-MODEL.md` v0; governance/funding decision; `DECISIONS.md` started.
Acceptance: a **successful unmodified Firefox build for all three targets** through the pipeline. (Prove the pipeline before changing anything.)
Risks: build-host setup friction; toolchain pinning; antivirus slowing/breaking Windows builds.

**Phase 1 — Rebrand + harden.**
Deliverables: branding patches; `autoconfig.js` + `openbook.cfg`; `policies.json`; first OpenBook build; privacy-regression suite.
Acceptance: builds on all targets; privacy suite green (telemetry off, RFP on, DoH default, WebRTC policy in place); no Firefox trademarks shipped.
Risks: locked-pref breakage; policy/pref drift across Firefox versions.

**Phase 2 — Cryptographic Lockout.**
Deliverables: Rust native host (Argon2id + hardware-bound key + crypto-erasure); encrypted profile container (chosen option); `vault-ui`; escalating delays; fuzzed parser; threat-model validation.
Acceptance: in the VM harness, N failures → verifiably unrecoverable data; counter survives power-cycle; correct file permissions; fallback path behaves and is labeled.
Risks: cross-platform hardware-store variance; profile-container integration; **never test key erasure on the host.**

**Phase 3 — Proxy/VPN manager.**
Deliverables: `proxy-manager` extension + policy/pref backing; fail-closed enforcement; leak-test harness; OS-WireGuard verification path.
Acceptance: leak suite green on all four vectors, including on tunnel failure.
Risks: WebRTC/DNS edge cases; IPv6 coverage.

**Phase 4 — AI sidebar.**
Deliverables: pluggable provider layer (local Ollama / BYO key); off-by-default flag; sandboxing; prompt-injection mitigations; zero-telemetry verification.
Acceptance: no network calls until opt-in; read-only default; per-action confirmation enforced; egress test green.
Risks: prompt-injection coverage; local-model UX/hardware assumptions.

**Phase 5 — Release engineering.**
Deliverables: signing for all three OSes (GPG repos / Authenticode / Developer ID + notarize + staple); packaging (deb/rpm/flatpak/appimage, dmg/pkg, exe/msi); update strategy; SBOM; checksums + signatures; reproducible-build diff test.
Acceptance: signed, installable artifacts on all targets; repro diff passes; published verification instructions.
Risks: macOS notarization/codesign pipeline; Windows EV cert procurement; repro determinism.

**Phase 6 — Security + sustainability.**
Deliverables: external-audit prep; disclosure policy + bug bounty; signing-key management plan; MFSA-tracking patch-maintenance automation; v2 track (Servo-embed prototype; full `rbm`-style reproducible builds).
Acceptance: audit-ready; automated upstream-security tracking; documented maintenance SLA.
Risks: maintenance burden sustainability; funding.

---

## 13. Legal / trademark

Firefox **code** is MPL 2.0 (disclose modifications, keep license headers). The Firefox **name and logo are Mozilla trademarks** — a fork **must rebrand** (you cannot ship it called Firefox). LibreWolf, Mullvad Browser, and Tor Browser all rebrand. OpenBook branding is therefore **mandatory,** not cosmetic. Confirm current Mozilla trademark/redistribution terms before release.

---

## 14. Resources and proof

**Grounded via current sources (this build plan's claims):**

- LibreWolf fork model (Firefox source tarball → patches + theming + build scripts + `librewolf.cfg`/`policies.json`; separate settings repo; Docker build → deb/rpm/Flatpak/AppImage/AUR/Chocolatey/MS Store): codeberg.org/librewolf/source ; codeberg.org/librewolf/settings ; github.com/pettinen/librewolf
- Firefox AutoConfig (`autoconfig.js` in `defaults/pref/`, `general.config.filename`, `pref`/`defaultPref`/`lockPref`, privileged JS, literals not strings): support.mozilla.org/kb/customizing-firefox-using-autoconfig ; mike.kaply.com/2016/09/08/debugging-firefox-autoconfig-problems/
- Firefox enterprise policies + templates: support.mozilla.org/kb/customizing-firefox-using-policiesjson ; github.com/mozilla/policy-templates
- Privileged-config-as-privesc finding (permissions invariant): mdsec.co.uk/2020/04/abusing-firefox-in-enterprise-environments/
- Build requirements (≥30 GB disk / 40 GB Windows, 8 GB+ RAM, Python 3.9+, `mach bootstrap`/`build`/`clobber`) and artifact builds (`--enable-artifact-builds`): firefox-source-docs.mozilla.org/setup/linux_build.html ; .../setup/windows_build.html ; .../setup/macos_build.html ; .../contributing/build/artifact_builds.html ; .../setup/configuring_build_options.html
- Native messaging (`nativeMessaging` perm, stdio host manifest, `allowed_extensions`, host not managed by browser): developer.mozilla.org/en-US/docs/Mozilla/Add-ons/WebExtensions/Native_messaging ; .../Native_manifests

**Verify against primary standards (security-engineering claims — confirm independently):**

- Argon2 parameters → RFC 9106.
- Password/PIN storage → OWASP Password Storage Cheat Sheet.
- Cryptographic erasure + why SSD overwrite is unreliable → NIST SP 800-88 Rev. 1 (Media Sanitization, "Cryptographic Erase"); Wei et al., "Reliably Erasing Data from Flash-Based Solid State Drives," USENIX FAST 2011.
- Hardware-enforced attempt limits + data protection → Apple Platform Security Guide (Secure Enclave, Data Protection — including failed-passcode erasure); Microsoft TPM fundamentals (dictionary-attack lockout).
- WebRTC IP-leak behavior → WebRTC / ICE specs and browser WebRTC docs.
- Reproducible builds → reproducible-builds.org ; Tor Project build (`rbm`) docs.

---

## Appendix A — `CLAUDE.md`

See `CLAUDE.md` in the repo root. It is the operational guardrail: invariants, repo map, the upstream mechanisms to use, conventions, per-phase definition of done.

## Appendix B — Implementation status

See `docs/DECISIONS.md` for the ADR log recording every non-trivial choice made while implementing this plan, and the per-component `README.md` files for the as-built detail of each layer.
