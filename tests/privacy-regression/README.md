<!-- SPDX-License-Identifier: MPL-2.0 -->

# Privacy-regression tests

Static, dependency-free guards on OpenBook's Phase 1 hardening config. They make
the zero-telemetry / hardened-defaults invariants **executable** so a regression
in `config/autoconfig/` or `config/policies/` fails before any build is made.

All tests are plain Python (stdlib only). Run any one directly:

```sh
python3 tests/privacy-regression/test_autoconfig_hardening.py
python3 tests/privacy-regression/test_autoconfig_js.py
python3 tests/privacy-regression/test_policies_json.py
python3 tests/privacy-regression/test_no_telemetry_endpoints.py
```

Each exits 0 on success and prints a one-line summary. They are also
pytest-discoverable (every check is a `test_*` function), so `pytest
tests/privacy-regression` works too.

## What each test guarantees

| File | Guarantee |
|---|---|
| `test_autoconfig_hardening.py` | Parses `openbook.cfg` (ignoring the always-ignored first line, stripping comments) and asserts the hardened baseline is present with correct **literal** values: telemetry off + locked, telemetry/coverage/normandy servers `""`, datareporting upload off, studies/normandy off, captive-portal/connectivity off, RFP on (as `defaultPref`), tracking protection on, `cookieBehavior 5`, DoH `trr.mode 2`, `socks_remote_dns` locked true, prefetch/predictor/speculative off, Pocket locked off, sponsored/first-run off, search suggestions off by default, HTTPS-only locked on, beacon/geo off. Also asserts the **first line is a comment** and that **no boolean is quoted** (`"false"`), catching the classic AutoConfig literals-not-strings bug. Crucially asserts WebRTC is **not** blanket-disabled in the cfg (proxy-manager owns that, §6). |
| `test_autoconfig_js.py` | `autoconfig.js` sets `general.config.filename = "openbook.cfg"`, `obscure_value = 0` (plaintext/auditable), and `sandbox_enabled = false` (so `lockPref` works). |
| `test_policies_json.py` | `policies.json` is **valid JSON** with top-level key `policies`; `DisableTelemetry`/`DisableFirefoxStudies`/`DisablePocket` true; first-run/post-update pages blank; `NoDefaultBookmarks` true; tracking protection on; UserMessaging recommendations off; FirefoxHome sponsored/Pocket off; DuckDuckGo default (user-changeable); the three OpenBook extensions allowlisted; DoH enabled; updates kept on. |
| `test_no_telemetry_endpoints.py` | Cross-cuts both shipped artifacts: every telemetry/normandy/coverage **server pref is `""`**, the master enable flags are false, and **no Mozilla telemetry hostname appears as an active value** in either the cfg (comments stripped) or the policy JSON. The static half of invariant #1. |

## The live first-run egress test

The static tests cannot prove a *running* build stays silent. That is the job of
the **live first-run network-egress test**, which launches a real OpenBook build
in a network-namespace / DNS+TLS sink and **fails on any outbound connection not
on an explicit allowlist**. It runs in **CI against a built artifact** (it needs
a binary and privileged networking), not in this repo. Its full design,
allowlist policy, and how it plugs into the Phase 1 acceptance gate are in
[`FIRST_RUN_EGRESS.md`](./FIRST_RUN_EGRESS.md).

Together: the static tests are the always-on tripwire on the config layer; the
live egress test is the ground-truth check on the built browser.
