<!-- SPDX-License-Identifier: MPL-2.0 -->

# First-run network-egress test (live, CI-only)

The static tests in this directory prove the **shipped config** enables no
telemetry endpoint. They cannot prove that a **real running build** makes no
unexpected outbound connection — that requires launching the actual browser and
watching the network. This document specifies that live test, which runs in CI
against a built OpenBook artifact (it is **not** runnable in this repo, which has
no build).

This is the operationalization of CLAUDE.md security invariant #1 ("Zero
telemetry: no unsolicited outbound connections; CI must include first-run egress
tests") and `docs/OpenBook-Browser-Build-Plan.md` §10.

## What it asserts

On the **very first launch** of a fresh profile (the moment Firefox normally
fires telemetry pings, captive-portal probes, Normandy fetches, etc.), OpenBook
must make **zero** outbound connections except those on an explicit allowlist.
Any connection not on the allowlist **fails the build**.

## Harness design

Run the browser in a controlled network where every egress attempt is observed
and, by default, denied:

1. **Network namespace + sink.** Launch the build inside a dedicated Linux
   network namespace (or a disposable container) whose only route is to a local
   collector. No real upstream connectivity.
2. **Sinkhole DNS + TLS sink.** Point all DNS at a local resolver that logs
   every query and answers with a loopback sink address. Run a TLS-terminating
   sink (mitm-style) on 443/80 that logs SNI + Host + path and never forwards.
   This captures both the intended destination (even for TLS) and the fact that
   a connection was attempted.
3. **Packet-level backstop.** Capture with `tcpdump`/`pcap` (or nftables logging)
   so even a raw socket or QUIC/UDP attempt that bypasses the HTTP sink is seen.
   Include UDP/443 (HTTP/3) and DoH.
4. **Drive first run.** Start the browser with a brand-new profile via Marionette
   (or `--headless` where supported), wait a fixed settle window (e.g. 60s)
   covering startup, idle telemetry timers, and the captive-portal/connectivity
   check windows. Optionally open one `about:` page; do **not** browse the web.
5. **Evaluate against an allowlist.** Collect every observed destination
   (hostname from DNS/SNI/Host, plus IP/port from pcap). Compare to the
   allowlist. **Fail on any connection not explicitly allowed.**

## Allowlist (deny-by-default)

The default allowlist is **empty** for a pure offline first run. The only
candidates ever permitted, and only when the corresponding feature is actually
exercised, are:

- The configured **DoH resolver** host (e.g. `dns.quad9.net`) — and only if the
  test intentionally triggers a DNS lookup. A bare first run with no navigation
  should not contact it.
- ~~The app-update check host~~ — **removed (ADR-0018):** the in-app updater is
  disabled (`policies.json` `DisableAppUpdate:true`, `openbook.cfg` §11) because
  OpenBook runs no update server and the compiled-in endpoint is Mozilla's.
  ANY `aus*.mozilla.org` contact is now a hard failure. Updates ship via the
  signed package channels (ADR-0013).
- The **documented exceptions** in `openbook.cfg` §12 (Remote Settings/OneCRL,
  Safe Browsing list updates, GMP fetches, AMO update checks for user-installed
  extensions) — each only when the run actually exercises the feature and only
  to its named host; a bare no-navigation first run should observe **none** of
  them inside the settle window.

Everything else — `*.telemetry.mozilla.org`, `normandy.*`, `*.services.mozilla.com`,
`detectportal.firefox.com`, `*.googleapis.com`, Safe Browsing update hosts during
a no-navigation run, Pocket, snippets, "what's new" — appearing in the capture is
a **hard failure**.

## Relationship to the static tests

| Test | Layer | Runs |
|---|---|---|
| `test_no_telemetry_endpoints.py` | config (cfg + policies) | now, every push |
| `test_autoconfig_hardening.py` | config (cfg prefs/locks) | now, every push |
| **first-run egress (this doc)** | running build | CI, against a built artifact |

The static tests are the fast tripwire that prevents a config regression from
ever reaching a build; the live egress test is the ground truth that the built
browser actually stays silent on first run.

## Implementation note (TODO — build host)

This harness requires a built OpenBook binary and elevated networking
(namespaces / nft / pcap), so it lives in the CI pipeline (`ci/`) and the
`tests/leak/` harness alongside the §6 leak tests, not in this stdlib-only
directory. Wire it into the Phase 1 acceptance gate
(`docs/OpenBook-Browser-Build-Plan.md` §12, Phase 1: "privacy suite green").
