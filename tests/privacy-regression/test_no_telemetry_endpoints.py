#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Privacy-regression: assert the SHIPPED config enables no telemetry endpoint.

This is the static half of the zero-telemetry invariant (CLAUDE.md #1). It scans
both shipped config artifacts:
  * config/autoconfig/openbook.cfg
  * config/policies/policies.json
and asserts:
  1. Every telemetry/data-submission *server* pref is the empty string (there is
     literally no host to phone home to).
  2. No known Mozilla telemetry/Normandy ingestion hostname appears as a value
     of an ENABLED pref. (A hostname may legitimately appear inside a comment,
     which is stripped before scanning.)
  3. The master enable flags are false.

The LIVE half — a process-level first-run network-egress test asserting ZERO
unexpected outbound connections on first launch of a real build — runs in CI
against an actual build, not here. See FIRST_RUN_EGRESS.md in this directory.
This static test is the always-on tripwire that catches a regression in the
config layer before a build is ever produced.

Pure stdlib; runnable with python3 and pytest-discoverable.
"""
from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CFG = ROOT / "config" / "autoconfig" / "openbook.cfg"
POLICIES = ROOT / "config" / "policies" / "policies.json"

# Hostnames that, if pointed at by an enabled pref, mean telemetry egress.
TELEMETRY_HOSTS = (
    "incoming.telemetry.mozilla.org",
    "telemetry.mozilla.org",
    "normandy.cdn.mozilla.net",
    "normandy.mozilla.org",
    "coverage.mozilla.org",
    "shavar.services.mozilla.com",  # only a concern if mis-wired as telemetry
    "experiments.mozilla.org",
    "ping.mozilla.org",
)

# Prefs whose VALUE is a submission endpoint; must be "" in the shipped config.
SERVER_PREFS = (
    "toolkit.telemetry.server",
    "toolkit.coverage.endpoint.base",
    "app.normandy.api_url",
)

_CALL = re.compile(
    r"""(?P<func>lockPref|defaultPref|pref)\s*\(\s*
        (["'])(?P<name>[^"']+)\2\s*,\s*(?P<value>.+?)\s*\)\s*;""",
    re.VERBOSE,
)


def _cfg_code_lines() -> list[str]:
    """cfg lines with the ignored first line dropped and // comments stripped."""
    lines = CFG.read_text(encoding="utf-8").splitlines()[1:]
    out = []
    for raw in lines:
        code = raw.split("//", 1)[0]
        if code.strip():
            out.append(code)
    return out


def _cfg_prefs() -> dict[str, str]:
    out: dict[str, str] = {}
    for code in _cfg_code_lines():
        for m in _CALL.finditer(code):
            out[m.group("name")] = m.group("value").strip()
    return out


CFG_PREFS = _cfg_prefs()


def test_server_prefs_are_empty_string() -> None:
    for name in SERVER_PREFS:
        assert name in CFG_PREFS, f"expected {name} to be set (to empty string)"
        assert CFG_PREFS[name] in ('""', "''"), (
            f"{name} must be the empty string (no endpoint), got {CFG_PREFS[name]!r}"
        )


def test_no_telemetry_host_enabled_in_cfg() -> None:
    """No telemetry hostname may appear as a pref VALUE in the cfg code (i.e.
    outside comments). Comments are already stripped in _cfg_code_lines()."""
    offenders = []
    for code in _cfg_code_lines():
        for host in TELEMETRY_HOSTS:
            if host in code:
                offenders.append(f"{host} -> {code.strip()}")
    assert not offenders, (
        "telemetry endpoint hostname present in active cfg (must be removed / "
        "blanked): " + " | ".join(offenders)
    )


def test_master_enable_flags_false_in_cfg() -> None:
    for name in (
        "toolkit.telemetry.enabled",
        "toolkit.telemetry.unified",
        "datareporting.healthreport.uploadEnabled",
        "datareporting.policy.dataSubmissionEnabled",
        "app.shield.optoutstudies.enabled",
        "app.normandy.enabled",
    ):
        assert CFG_PREFS.get(name) == "false", (
            f"{name} must be false in openbook.cfg, got {CFG_PREFS.get(name)!r}"
        )


def test_policies_disable_telemetry_and_no_host() -> None:
    data = json.loads(POLICIES.read_text(encoding="utf-8"))
    pol = data["policies"]
    assert pol.get("DisableTelemetry") is True
    assert pol.get("DisableFirefoxStudies") is True
    # No telemetry hostname anywhere in the (comment-free) policy JSON text.
    blob = json.dumps(data)
    for host in TELEMETRY_HOSTS:
        assert host not in blob, f"telemetry host {host} present in policies.json"


def main() -> None:
    fns = [v for k, v in sorted(globals().items())
           if k.startswith("test_") and callable(v)]
    for fn in fns:
        fn()
    print(f"OK: no telemetry endpoints enabled in shipped config ({len(fns)} checks). "
          "Live first-run egress test runs in CI (see FIRST_RUN_EGRESS.md).")


if __name__ == "__main__":
    main()
