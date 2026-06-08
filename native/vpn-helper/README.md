<!-- SPDX-License-Identifier: MPL-2.0 -->

# `org.openbook.vpn_helper` — exit-IP verification host (DEFERRED scaffold)

> **Status: DEFERRED scaffold (Build Plan §6).** This crate compiles, tests, and
> speaks the native-messaging protocol, but it deliberately implements **only**
> exit-IP verification and the networked probe step is **stubbed**. It does
> **not** create or manage tunnels and ships **no** bundled WireGuard/VPN stack.

## What this is (and is not)

OpenBook's position (Build Plan §6) is that **a browser is the wrong layer to run
a full system VPN tunnel** — that is kernel/OS networking. The browser owns
per-profile proxying and leak prevention, not tunnel creation.

The four-way decision in §6 lands on:

- **Supported real-tunnel model — §6 Option 1: "OS-level VPN, browser verifies."**
  The user runs WireGuard/OpenVPN (or any OS VPN). The browser's only job is to
  optionally **verify the exit IP** so the user can confirm traffic is leaving
  where they expect.
- **§6 Option 3 (bundled userspace WireGuard) is explicitly rejected for v1** —
  it would mean shipping a network stack and handling a privileged `tun` device.
- Accordingly the `vpn-helper` itself is **deferred**: only the verification
  helper is scaffolded here. The leak-prevention enforcement (WebRTC/DNS/IPv6/
  fail-closed) lives in the `proxy-manager` extension plus the pref/policy layer,
  not in this host.

This host therefore:

- **DOES** speak the Firefox native-messaging protocol (status + verify).
- **DOES** perform a fully **offline** exit-IP comparison when the caller supplies
  both an expected and an observed IP.
- **DOES NOT** create, configure, or tear down any tunnel.
- **DOES NOT** ship or embed WireGuard, `boringtun`, `wireguard-go`, or any VPN
  client.
- **DOES NOT** make any network call at rest. The single would-be networked step
  — fetching the live exit IP — is **stubbed**; see "Verify semantics" below.

## Permissions / network invariant

- **No outbound traffic at rest.** The dispatch path (`protocol::dispatch`) is
  pure: no sockets, no filesystem. `status` returns a static descriptor; `verify`
  either does an in-memory string compare or returns
  `not-implemented-in-scaffold`. There is no code path in this scaffold that
  opens a network connection. This is what makes the zero-telemetry / no-egress
  invariant trivially true and unit-testable here.
- **A live probe, if ever implemented, must be explicit and user-driven** — never
  background, never on launch — and must be added on a real build host as a
  reviewed change with its own egress tests.
- **Native-host security model.** The browser validates this host's manifest but
  does **not** install or manage it (it is a native application). Per the §11
  permissions invariant, the installed binary
  (`/usr/lib/openbook/org.openbook.vpn_helper`) and its manifest
  (`org.openbook.vpn_helper.json`) **must be root-owned and not user-writable** in
  release packages. A user-writable native host is a local privilege-escalation
  hole.
- **`allowed_extensions`** is restricted to `proxy-manager@openbook.browser`; only
  the proxy-manager extension may connect.

## Wire protocol

Framing is identical to the vault host: a **4-byte native-endian length** prefix
followed by that many bytes of **UTF-8 JSON**. A single message is capped at
**1 MiB**; an over-large declared length is rejected as `invalid-request` without
allocating the claimed size. Malformed input never panics — it yields a clean
`invalid-request` and the desynchronized stream is closed.

### Requests

```jsonc
// Liveness / capability probe.
{ "type": "status", "id": 1 }

// Exit-IP verification. expectedExitIp / observedExitIp are both optional.
{ "type": "verify", "id": 2, "expectedExitIp": "203.0.113.7", "observedExitIp": "203.0.113.7" }
```

### `verify` semantics (scaffold)

| `expectedExitIp` | `observedExitIp` | Result |
|---|---|---|
| present | present | **offline** string compare → `outcome: "match"` or `"mismatch"` (`performedNetworkProbe: false`) |
| present | absent | `outcome: "not-implemented-in-scaffold"` — the live probe is the stubbed networked step |
| absent | absent | `outcome: "not-implemented-in-scaffold"` — treated as a capability query |

**Fail-closed consumption rule:** the `proxy-manager` extension must treat
**anything other than an explicit `"match"`** — including `"mismatch"` and
`"not-implemented-in-scaffold"` — as "exit IP not verified" and must **not** assume
the tunnel is up.

### Responses

```jsonc
// status
{ "id": 1, "ok": true, "type": "status", "host": "org.openbook.vpn_helper",
  "role": "exit-ip-verification", "deferred": true, "createsTunnels": false,
  "shipsWireguard": false, "performsNetworkAtRest": false,
  "model": "os-level-vpn-browser-verifies", "message": "..." }

// verify (offline compare)
{ "id": 2, "ok": true, "type": "verify", "outcome": "match", "matches": true,
  "expectedExitIp": "203.0.113.7", "observedExitIp": "203.0.113.7",
  "performedNetworkProbe": false, "message": "..." }

// error (malformed input, unknown type)
{ "id": 0, "ok": false, "error": "invalid-request", "message": "..." }
```

## Build and test

```sh
cargo build --manifest-path native/vpn-helper/Cargo.toml
cargo test  --manifest-path native/vpn-helper/Cargo.toml
```

Dependencies are intentionally minimal (`serde`, `serde_json`) so the build is
offline-fast and links no network client. No feature flags are required.

## Installation note (release packaging)

The compiled binary is installed at the manifest's `path`
(`/usr/lib/openbook/org.openbook.vpn_helper` on Linux; per-OS native-messaging
manifest locations apply on Windows/macOS). Both the binary and the manifest must
be installed **root-owned and not user-writable** (Build Plan §11). Adjust `path`
per platform during packaging.

## Roadmap to undeferral

When the verification helper graduates from scaffold:

1. Implement the live exit-IP probe as an **explicit, user-triggered** request
   only, behind a reviewed change with dedicated egress tests in CI.
2. Keep the "no tunnel creation in the browser" stance — verification only.
3. Reassess §6 Option 1 vs. a maintained companion only if a clear, leak-safe
   design emerges; bundled userspace WireGuard remains rejected for v1.
