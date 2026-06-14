<!-- SPDX-License-Identifier: MPL-2.0 -->
# OpenBook Proxy (`proxy-manager`)

Phase 3 per-profile proxy with **fail-closed** enforcement and the four
mandatory leak controls (build plan §6). A bundled, first-party WebExtension.

- Manifest: V2, id `proxy-manager@openbook.browser`, Firefox 145+.
- Permissions: `proxy`, `privacy`, `webRequest`, `webRequestBlocking`, `dns`,
  `storage`, `<all_urls>`.
- No telemetry. No outbound connections except the health-check probe, which
  goes **through the configured proxy**.

## Non-goal (explicit)

A browser is **not** a system VPN — that is kernel/OS networking. Per §6 the
supported real tunnel is **Option 1: OS-level WireGuard/OpenVPN**; this
extension implements **Option 2: per-profile SOCKS5/HTTP proxy + leak
prevention**, which is only safe with strict fail-closed + all four leak
controls. It points at *your own* endpoint; it ships no proxy/VPN service.

## The four leak controls — enforced WHERE

| # | Control | Enforced by | Backing pref/policy |
|---|---|---|---|
| 1 | **WebRTC** | This extension via `browser.privacy.network` — disables `peerConnectionEnabled` and sets `webRTCIPHandlingPolicy = disable_non_proxied_udp` whenever a proxy is active or the kill-switch is on, so ICE cannot expose the real IP. | (extension-enforced) |
| 2 | **DNS** | `proxy.onRequest` returns `proxyDNS:true` for SOCKS so DNS resolves at the proxy, not the local OS resolver. | `network.proxy.socks_remote_dns` (autoconfig/policy) |
| 3 | **IPv6** | Surfaced as a warning + setting. For a v4-only proxy, native IPv6 can bypass the tunnel; the extension warns. | `network.dns.disableIPv6` (autoconfig/policy) |
| 4 | **Fail-closed** | A **blocking** `webRequest.onBeforeRequest` listener that CANCELS every request whenever `decideRequest(state).cancel` is true (kill-switch engaged with no usable proxy, or proxy enabled but health-check failing/degraded/unknown). A periodic probe through the proxy flips health. **Never silently falls back to direct.** | (extension-enforced) |

Controls 2 and 3 are **pref/policy-backed** because an extension cannot set
those network prefs; the `proxy-manager` UI surfaces and explains them, and the
OpenBook autoconfig/policy layer (`config/`) sets the actual values.

## Fail-closed model (security invariant 2)

`decideRequest(state)` is a **pure** function (`src/failclosed.ts`) — the single
source of truth, fully unit-tested:

- kill-switch OFF and proxy OFF → allow (plain direct browsing).
- proxy ON **and** `health === "healthy"` → allow (routed through proxy).
- otherwise (kill-switch on with no usable proxy, or proxy on but
  unhealthy/unknown) → **cancel**. Uncertainty blocks.

`nextHealthState(prev, probe)` advances health: `ok → healthy` (reset failures),
`fail → degraded` then `failing` at the `FAILURE_THRESHOLD`, `unknown` preserves
prior state without granting a positive signal. Reconfiguring the proxy resets
health to `unknown`, so traffic is blocked until the path is re-proven.

The default state is fail-closed: `killSwitch:true`, proxy unconfigured ⇒
traffic blocked until the user configures and the probe succeeds.

## Health-check loop

`background.ts` fetches a check endpoint every 15s **through the proxy** (HEAD,
5s timeout). Success → `healthy`; failure/timeout → increments failures →
`degraded`/`failing` → fail-closed blocks. The probe only runs when a proxy is
enabled and configured.

## Files

- `src/types.ts` — state + config types, defaults, `FAILURE_THRESHOLD`.
- `src/failclosed.ts` — **pure**: `decideRequest`, `nextHealthState`,
  `displayStatus`, `isValidEndpoint`.
- `src/proxy.ts` — `proxy.onRequest` handler + pure `resolveProxyInfo`
  (SOCKS `proxyDNS:true`).
- `src/leakcontrols.ts` — pure `webrtcPolicyFor` / `ipv6WarningFor` +
  side-effecting `applyWebRtcPolicy`.
- `src/background.ts` — wires the proxy handler, the blocking fail-closed
  listener, the health-check loop, WebRTC application, and persistence.
- `popup.html` / `src/popup.ts` / `popup.css` — configure host/port/type,
  toggle the kill-switch, show status (protected / blocked / leaky / direct).
- `src/__tests__/` — jest unit tests.

## Build / test

```sh
npm install
npm run build   # tsc -p tsconfig.json (strict, zero errors)
npm test        # jest
npm run lint    # web-ext lint -s dist (best-effort; needs a built dist)
```

## TODO (needs a real Firefox build)

- `tests/leak/` harness: run the browser against a controlled proxy and assert
  no traffic escapes on WebRTC/DNS/IPv6, **including on tunnel failure** (the
  fail-closed assertion). That requires web-ext/Marionette and is out of scope
  for jest.
- Wire the OS-WireGuard exit-IP verification path (Option 1) — a separate
  surface from this per-profile proxy.
- The health-check URL is **user-supplied per profile** — OpenBook ships no
  default probe endpoint (a hardcoded one would be unsolicited egress to a third
  party, invariant 1). Until the user sets one and a probe succeeds through the
  proxy, fail-closed keeps traffic blocked and the popup says why.
