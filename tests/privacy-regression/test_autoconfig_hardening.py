#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Privacy-regression: parse config/autoconfig/openbook.cfg and assert the
hardened baseline is present with correct *literal* values.

This is the core Phase 1 guard. It catches:
  * a missing/regressed hardening pref,
  * the "literals not strings" bug (lockPref("x", "false") instead of false),
  * a first line that is not a comment (the AutoConfig parser ignores line 1,
    so a real pref there silently does nothing).

Pure stdlib; runnable as `python3 tests/privacy-regression/test_autoconfig_hardening.py`
and also pytest-discoverable via the test_* functions.
"""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CFG = ROOT / "config" / "autoconfig" / "openbook.cfg"

# Tolerant matcher for AutoConfig calls:
#   <func>("pref.name", <value>);
# value is captured raw (literal) up to the closing paren.
_CALL = re.compile(
    r"""(?P<func>lockPref|defaultPref|pref|clearPref)\s*\(\s*
        (?P<q>["'])(?P<name>[^"']+)(?P=q)\s*
        (?:,\s*(?P<value>.+?)\s*)?\)\s*;""",
    re.VERBOSE,
)


def _body_lines() -> list[str]:
    """All lines of the cfg EXCEPT the always-ignored first line."""
    text = CFG.read_text(encoding="utf-8")
    return text.splitlines()[1:]


def parse_prefs() -> dict[str, tuple[str, str]]:
    """Return {pref_name: (func, raw_value_literal)} for the last assignment
    of each pref. Comments (// ...) are stripped first so commented-out example
    prefs never count as set. The first line is excluded per AutoConfig rules."""
    result: dict[str, tuple[str, str]] = {}
    for raw in _body_lines():
        line = raw.strip()
        if line.startswith("//"):
            continue
        # Drop a trailing line comment but keep string contents intact enough
        # for our literal checks (our prefs do not embed "//" in values).
        code = line.split("//", 1)[0]
        for m in _CALL.finditer(code):
            name = m.group("name")
            value = (m.group("value") or "").strip()
            result[name] = (m.group("func"), value)
    return result


PREFS = parse_prefs()


def _val(name: str) -> str:
    assert name in PREFS, f"missing pref: {name}"
    return PREFS[name][1]


def _func(name: str) -> str:
    assert name in PREFS, f"missing pref: {name}"
    return PREFS[name][0]


def test_first_line_is_comment() -> None:
    first = CFG.read_text(encoding="utf-8").splitlines()[0].strip()
    assert first.startswith("//"), (
        "AutoConfig ignores line 1; it MUST be a comment, got: %r" % first
    )


def test_no_string_literal_booleans() -> None:
    """The classic AutoConfig bug: quoting a boolean. "false"/"true" are truthy
    strings. No pref in the file may have a quoted boolean as its value."""
    offenders = []
    for name, (func, value) in PREFS.items():
        v = value.strip()
        if v in ('"true"', "'true'", '"false"', "'false'"):
            offenders.append(f"{func}({name!r}, {value})")
    assert not offenders, (
        "string-literal booleans found (must be bare true/false): "
        + "; ".join(offenders)
    )


def test_telemetry_disabled_and_locked() -> None:
    assert _val("toolkit.telemetry.enabled") == "false"
    assert _func("toolkit.telemetry.enabled") == "lockPref"
    assert _val("toolkit.telemetry.unified") == "false"
    # Server endpoint must be the empty string (nowhere to phone home).
    assert _val("toolkit.telemetry.server") in ('""', "''")
    assert _func("toolkit.telemetry.server") == "lockPref"
    assert _val("toolkit.telemetry.archive.enabled") == "false"


def test_datareporting_off() -> None:
    assert _val("datareporting.healthreport.uploadEnabled") == "false"
    assert _val("datareporting.policy.dataSubmissionEnabled") == "false"


def test_studies_and_normandy_off() -> None:
    assert _val("app.shield.optoutstudies.enabled") == "false"
    assert _val("app.normandy.enabled") == "false"
    assert _val("app.normandy.api_url") in ('""', "''")


def test_coverage_endpoint_blank() -> None:
    assert _val("toolkit.coverage.endpoint.base") in ('""', "''")


def test_discovery_and_newtab_telemetry_off() -> None:
    assert _val("browser.discovery.enabled") == "false"
    assert _val("browser.newtabpage.activity-stream.feeds.telemetry") == "false"
    assert _val("browser.newtabpage.activity-stream.telemetry") == "false"


def test_captive_portal_and_connectivity_off() -> None:
    assert _val("network.captive-portal-service.enabled") == "false"
    assert _val("network.connectivity-service.enabled") == "false"


def test_resist_fingerprinting_present() -> None:
    # RFP is ON; it is a defaultPref (user-overridable) by design.
    assert _val("privacy.resistFingerprinting") == "true"
    assert _func("privacy.resistFingerprinting") == "defaultPref"
    assert _val("privacy.fingerprintingProtection") == "true"


def test_tracking_protection_enabled() -> None:
    assert _val("privacy.trackingprotection.enabled") == "true"
    assert _val("privacy.trackingprotection.socialtracking.enabled") == "true"
    # Total Cookie Protection / dFPI behavior.
    assert _val("network.cookie.cookieBehavior") == "5"


def test_dns_and_connection_hardening() -> None:
    # DoH default mode 2 (preferred + fallback), user-changeable.
    assert _val("network.trr.mode") == "2"
    assert _func("network.trr.mode") == "defaultPref"
    # SOCKS remote DNS must be locked true (no DNS leak around a SOCKS proxy).
    assert _val("network.proxy.socks_remote_dns") == "true"
    assert _func("network.proxy.socks_remote_dns") == "lockPref"
    assert _val("network.dns.disablePrefetch") == "true"
    assert _val("network.predictor.enabled") == "false"
    assert _val("network.prefetch-next") == "false"
    assert _val("browser.urlbar.speculativeConnect.enabled") == "false"


def test_pocket_disabled_and_locked() -> None:
    assert _val("extensions.pocket.enabled") == "false"
    assert _func("extensions.pocket.enabled") == "lockPref"


def test_sponsored_and_firstrun_off() -> None:
    assert _val("browser.newtabpage.activity-stream.showSponsored") == "false"
    assert _val("browser.shopping.experience2023.enabled") == "false"
    assert _val("browser.aboutwelcome.enabled") == "false"


def test_search_suggestions_default_off() -> None:
    assert _val("browser.search.suggest.enabled") == "false"
    assert _func("browser.search.suggest.enabled") == "defaultPref"
    assert _val("browser.urlbar.suggest.quicksuggest.sponsored") == "false"


def test_webrtc_conservative_default_only() -> None:
    # proxy-manager owns WebRTC policy (§6). The .cfg must NOT blanket-disable
    # media.peerconnection.enabled, only set the conservative address default.
    assert _val("media.peerconnection.ice.default_address_only") == "true"
    assert "media.peerconnection.enabled" not in PREFS, (
        "WebRTC must not be blanket-disabled in openbook.cfg; proxy-manager "
        "owns that policy per docs §6"
    )


def test_misc_privacy() -> None:
    assert _val("beacon.enabled") == "false"
    # Geo off by default, user-overridable.
    assert _val("geo.enabled") == "false"
    assert _func("geo.enabled") == "defaultPref"
    # HTTPS-only locked on.
    assert _val("dom.security.https_only_mode") == "true"
    assert _func("dom.security.https_only_mode") == "lockPref"


def test_in_app_updater_disabled() -> None:
    # ADR-0018: OpenBook runs no update server; the compiled-in endpoint is
    # Mozilla's. The in-app updater must therefore be OFF (unsolicited egress +
    # would offer stock Firefox MARs). Security updates flow via the package
    # channels (ADR-0013) on the MFSA-tracked 1–2 day SLA.
    assert _val("app.update.auto") == "false"
    assert _func("app.update.auto") == "lockPref"
    assert _val("app.update.background.scheduling.enabled") == "false"
    assert _func("app.update.background.scheduling.enabled") == "lockPref"
    # And the cfg must keep saying WHY + where updates come from instead.
    text = CFG.read_text(encoding="utf-8")
    assert "ADR-0018" in text and "package" in text.lower(), (
        "openbook.cfg must document the package-channel update path"
    )


def test_remaining_background_egress_controlled() -> None:
    # Invariant 1: every fresh-profile endpoint is disabled or documented.
    # System add-on pipeline (silent privileged code swaps) must be hard-off.
    assert _val("extensions.systemAddon.update.enabled") == "false"
    assert _func("extensions.systemAddon.update.enabled") == "lockPref"
    assert _val("extensions.systemAddon.update.url") == '""'
    assert _func("extensions.systemAddon.update.url") == "lockPref"
    # The documented-exceptions block must name each deliberate exception.
    text = CFG.read_text(encoding="utf-8")
    assert "DOCUMENTED EXCEPTIONS" in text
    for exception in ("Remote Settings", "Safe Browsing", "gmp", "DoH"):
        assert exception in text, f"egress exception not documented: {exception}"


def main() -> None:
    assert CFG.is_file(), f"missing {CFG}"
    fns = [v for k, v in sorted(globals().items())
           if k.startswith("test_") and callable(v)]
    for fn in fns:
        fn()
    print(f"OK: openbook.cfg hardening verified ({len(PREFS)} prefs parsed, "
          f"{len(fns)} checks).")


if __name__ == "__main__":
    main()
