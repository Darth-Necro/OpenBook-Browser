<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook Browser changelog

OpenBook releases are versioned `<upstream-firefox-version>-<openbook-build>`
(ADR-0017), e.g. `145.0.2-1` is the first OpenBook build over Firefox 145.0.2.
Release tags are `v<version>`. Bundled components (extensions, native hosts)
carry their own semantic versions, listed per release.

## 145.0.2-1 (unreleased)

First OpenBook release line, over pinned and verified Firefox 145.0.2
(ADR-0001).

### Browser layer

- Ordered patch series: branding (OpenBook brand directory, neutral default
  bookmarks/start page), privacy (telemetry endpoints removed, Pocket
  component disabled), features (bundled system extensions, native messaging
  host registration).
- AutoConfig hardening layer (`openbook.cfg`, 113 prefs): telemetry, data
  reporting, Normandy, studies, and captive-portal phone-home off and locked;
  `privacy.resistFingerprinting` on by default but user-changeable
  (ADR-0014); DoH default; SOCKS remote DNS.
- Enterprise policy layer (`distribution/policies.json`, 35 policies)
  mirroring the telemetry/studies/Pocket posture as defense in depth.
- Branding: OpenBook name, icons, and strings; no Firefox trademarks shipped.

### Bundled components

- `vault-host` 0.1.0 (Rust): cryptographic lockout — Argon2id (RFC 9106
  params), hardware-bound KEK (TPM 2.0 / Secure Enclave providers
  feature-gated; labeled software fallback), monotonic pre-attempt counter,
  cryptographic erasure by key invalidation, fuzzed native-messaging parser.
- `vpn-helper` 0.1.0 (Rust): exit-IP verification scaffold (status/verify).
- `vault-ui` 0.1.0: setup wizard with no-recovery acknowledgement, lock
  screen, attempt feedback with escalating delays.
- `proxy-manager` 0.1.0: per-profile proxy with fail-closed kill-switch and
  the four leak controls (WebRTC, DNS, IPv6, fail-closed).
- `ai-sidebar` 0.1.0: opt-in, off-by-default assistant; pluggable providers
  (local Ollama default, BYOK with egress warning); read-only by default;
  per-action confirmation; prompt-injection guard.

### Release engineering

- Fail-hard pipeline: fetch → verify (Mozilla SHA-256 manifest + detached
  signature) → patch → build → test → package → sign → publish.
- Offline CI gates: privacy regression, leak/fail-closed simulation,
  reproducible-diff self-test, native + extension suites.
- Tag-driven release workflow producing component artifacts (extension XPIs,
  linux-x64 native hosts, settings overlay bundle), CycloneDX SBOM, and
  SHA256SUMS as a draft release; signing stays on maintainer hardware
  (ADR-0017).

### Security fixes from the pre-release audit

- vault-host: a malformed attempt-counter file now fails closed to the erased
  state instead of reading as a fresh budget; counter/secret writes are atomic
  (temp + rename + dir fsync) so a power loss can never truncate them; `unlock`
  on an unlocked vault is a real re-authentication under the counter policy.
- vault-ui: native-messaging `id` is numeric per the host's i64 wire contract
  (string ids made every vault request time out); added the missing `idle`
  permission so auto-lock arms.
- proxy-manager: kill-switch and proxy handlers now register synchronously at
  startup (closing a session-restore leak window); the health probe is routed
  through the proxy and exempted from self-cancellation via a per-probe nonce
  (previously health could never become healthy); health-check URL is
  user-supplied — OpenBook ships no default probe endpoint; dropped the unused
  `dns` permission.
- ai-sidebar: the "local" Ollama provider refuses non-loopback endpoints.
- Updater: in-app updater disabled (ADR-0018) — it would poll Mozilla's AUS
  and could serve stock Firefox; updates ship via signed package channels.
  System-add-on pipeline locked off; remaining endpoints documented in
  `openbook.cfg` §12; release mozconfigs compile out the crash reporter and
  updater.
- Packaging/signing scripts: unimplemented per-OS recipes now fail closed
  instead of reporting success; `mach package` gets the right MOZCONFIG; the
  packaging manifest patch genuinely adds the hardening layer + distribution
  XPIs (it previously listed them as context, which could never apply);
  obsolete patches dropped (Pocket is gone upstream; bundled extensions use
  `distribution/extensions/`); patch application enforces `--fuzz=0`; the
  AutoConfig sandbox stays enabled (least privilege); §11 permission checks
  cover parent directories.
