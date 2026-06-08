<!-- SPDX-License-Identifier: MPL-2.0 -->

# `tests/leak/` — proxy/VPN leak harness (Build Plan §6)

The proxy/VPN manager is only real if **all four** leak controls hold; with any
one missing it is "theater" (§6). This directory has two **offline gates** that
run in CI today and a documented **live harness** that runs the real browser.

## The four mandatory leak controls (§6)

1. **WebRTC** — disable, or force through the proxy; ICE candidates must never
   expose the real IP.
2. **DNS** — resolve through the proxy/tunnel (`network.proxy.socks_remote_dns=true`)
   and/or DoH (`network.trr.mode`); never let the OS resolver bypass it.
3. **IPv6** — ensure the tunnel covers IPv6, or disable IPv6 when the tunnel is
   v4-only. (Tunnel-state-dependent, like WebRTC — may be enforced by
   `proxy-manager`.)
4. **Fail-closed** — if the proxy/tunnel drops, **block** traffic; never silently
   go direct.

## Offline gates (run in CI now)

### `failclosed_sim.py` — control #4, deterministic, no external network

A self-contained simulation. It binds two local TCP listeners on `127.0.0.1`:

- a **direct-internet sink** that counts inbound connections (a stand-in for "the
  open internet you would reach if you bypassed the proxy"), and
- a **proxy/tunnel** listener that we can bring up or tear down.

It then exercises a `FailClosedClient` that routes only through the proxy:

- **Case A (tunnel up):** the request is served by the proxy; the direct sink
  stays at **zero** connections.
- **Case B (tunnel down):** every request is **blocked**; the direct sink stays at
  **zero** new connections — proving no silent direct fallback.
- **Case C (negative control):** a deliberately **fail-open** client *does* reach
  the sink and increments its counter — proving the harness can actually observe
  a leak, so Case B's assertions are meaningful and not vacuous.

```sh
python3 tests/leak/failclosed_sim.py     # exits 0 on success
```

This proves the fail-closed **logic** deterministically and offline. It does not
prove the *browser* fails closed — that is the live harness below.

### `leak_assertions.py` — static config gate for all four controls

Reads the settings layer and the proxy-manager extension and asserts the four
controls are *configured*. Inputs are produced by other tracks:

- `config/autoconfig/openbook.cfg`
- `config/policies/policies.json`
- `extensions/proxy-manager/` source

Resolution policy per control:

- satisfied by **any** present source → **PASS**;
- satisfied by **none**, and the source that owns it has **not landed** → **SKIP**
  with a clear message (e.g. IPv6 and WebRTC enforcement legitimately live in
  `proxy-manager`, whose enable/disable decision depends on tunnel state);
- satisfied by **none**, and the owning source **has** landed content → **FAIL**
  (a real, complete gap before release).

Assertions are **tolerant** (they search for the relevant keys/intent, not exact
values) so they survive reasonable changes by the other tracks.

```sh
python3 tests/leak/leak_assertions.py    # exits 0 on pass or graceful skip
```

Both files expose `test_*` functions and are pytest-discoverable
(`pytest tests/leak/`) as well as runnable directly with `python3`.

## The live harness (runtime gate — documented, runs on a real build)

The offline gates above cannot prove the *browser engine* obeys the controls;
that requires driving the real OpenBook build. The live harness (run on a build
host / CI runner that has produced an OpenBook binary) does the following:

1. **Controlled environment.** Stand up:
   - a **SOCKS5/HTTP proxy** under test control that logs every connection, and
   - a **sinkhole / blackhole** for the "direct" path: a DNS server that answers
     with controlled addresses and a packet filter (e.g. `iptables`/`nftables` or
     a network namespace) that captures or drops any packet that tries to leave
     by a route other than the proxy. Anything that reaches the sinkhole is a
     leak.
2. **Drive the browser** with **Marionette** or **Playwright-Firefox** against a
   throwaway profile configured to use the test proxy via `proxy-manager`.
3. **Assert per vector:**
   - **WebRTC:** load a page that creates an `RTCPeerConnection` and gathers ICE
     candidates; assert no candidate exposes a real local/public IP (only
     proxy-safe / mDNS-obfuscated candidates), or that WebRTC is disabled.
   - **DNS:** trigger navigations/lookups; assert **all** DNS egress is observed
     at the proxy/DoH endpoint and **none** at the OS resolver / sinkhole.
   - **IPv6:** repeat on a dual-stack host; assert no IPv6 traffic escapes the
     tunnel (either it is carried by the tunnel or v6 is disabled).
   - **Fail-closed:** with traffic flowing through the proxy, **kill the proxy**
     mid-session and assert subsequent requests are **blocked** (no packet reaches
     the sinkhole) — the live analogue of `failclosed_sim.py` Case B.
4. **Egress capture as ground truth.** A packet capture on every non-proxy
   interface must show **zero** unexpected outbound packets across the whole run
   (ties into the §10 first-run egress test).

The harness fails the build on any escaped packet on any vector. Until that
infrastructure runs in CI, `failclosed_sim.py` (logic) and `leak_assertions.py`
(configuration) are the enforced gates.

## Destructive-work note

The live harness manipulates packet filters / network namespaces and must run in
a **disposable VM/container against a throwaway profile**, never the developer
host (CLAUDE.md invariant #7).
