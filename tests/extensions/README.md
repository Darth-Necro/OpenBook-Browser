<!-- SPDX-License-Identifier: MPL-2.0 -->
# Extension tests

Cross-extension test orchestration for the three bundled OpenBook
WebExtensions (`extensions/vault-ui`, `extensions/proxy-manager`,
`extensions/ai-sidebar`).

## What runs here vs. where

### Unit tests (jest) — runnable now, no browser

Each extension owns its jest suite under `extensions/<name>/src/__tests__/`.
They test the **pure logic** that was deliberately extracted so it has no
`browser.*` calls and needs no network or native host. `run.sh` builds
(`tsc`) and runs all three.

```sh
tests/extensions/run.sh
# or per extension:
npm --prefix extensions/vault-ui test
npm --prefix extensions/proxy-manager test
npm --prefix extensions/ai-sidebar test
```

Coverage today:

- **vault-ui** — native-messaging protocol request/response (de)serialization;
  `VaultClient` id-correlation (out-of-order responses, error responses,
  malformed-frame drop, disconnect fail-safe, timeout); the software-mode
  weak-secret rule (`evaluateSecretStrength`); lock-screen presentation helpers.
- **proxy-manager** — `decideRequest` (fail-closed): blocks when the kill-switch
  is engaged with no usable proxy, or when the proxy is enabled but
  health is failing/degraded/unknown, and allows only when healthy + proxy set;
  `nextHealthState` transitions; `displayStatus`; endpoint validation;
  `resolveProxyInfo` (SOCKS `proxyDNS:true`); `webrtcPolicyFor` / `ipv6WarningFor`.
- **ai-sidebar** — registry returns **no active provider** when disabled
  (off-by-default ⇒ no possible network); BYOK requires an explicit egress
  acknowledgement; `promptguard` wraps page content as untrusted data and
  defuses forged delimiters; an action with `requiresConfirmation` does not run
  without an explicit confirm; provider streaming parsers (Ollama NDJSON, BYOK
  SSE) via an injected fake fetch.

### Integration tests — need a real Firefox build (NOT run here)

These require a built OpenBook/Firefox and live tooling, so they are out of
scope for the jest unit run:

- **web-ext / Marionette / Playwright-Firefox** loading each extension into a
  real profile: lock-screen flow against the native host, popup-driven proxy
  config, sidebar opt-in.
- **`tests/leak/`** — the §6 leak harness: run the browser against a controlled
  proxy and assert no traffic escapes on WebRTC / DNS / IPv6, **including on
  tunnel failure** (the fail-closed assertion).
- **`tests/native/`** — the vault native host (`org.openbook.vault_host`) with
  its destructive flows (setup/unlock-to-exhaustion/erase) exercised **only**
  against disposable VMs/containers and throwaway profiles (security invariant
  7). The extension jest suites never invoke the host or perform erasure.
- **`tests/privacy-regression/`** — first-run egress test asserting zero
  unexpected outbound connections (the AI off-by-default proof at the network
  layer).

## Notes

- `run.sh` is `set -euo pipefail` and resolves paths from its own location, so
  it works regardless of the caller's working directory. It installs deps for an
  extension only if `node_modules` is missing.
- The shipped `dist/*.js` page entry points are ES modules (loaded via
  `<script type="module">` / a background `page`); the jest runner transpiles
  to CommonJS internally and maps `.js` import specifiers back to `.ts`.
