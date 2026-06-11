#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Privacy-regression: validate config/policies/policies.json.

Asserts the file is strictly valid JSON, the top-level key is "policies", and
the privacy-critical policies are present with the expected values (defense in
depth duplicating the AutoConfig locks). Pure stdlib; runnable with python3 and
pytest-discoverable.
"""
from __future__ import annotations

import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
POLICIES = ROOT / "config" / "policies" / "policies.json"


def load() -> dict:
    return json.loads(POLICIES.read_text(encoding="utf-8"))


DATA = load()
P = DATA.get("policies", {})


def test_is_valid_json_with_policies_root() -> None:
    # load() above already proves it parses; assert structure here.
    assert isinstance(DATA, dict)
    assert "policies" in DATA, 'top-level key must be "policies"'
    assert isinstance(P, dict)


def test_telemetry_and_studies_disabled() -> None:
    assert P.get("DisableTelemetry") is True
    assert P.get("DisableFirefoxStudies") is True


def test_pocket_disabled() -> None:
    assert P.get("DisablePocket") is True


def test_default_browser_agent_off() -> None:
    assert P.get("DisableDefaultBrowserAgent") is True
    assert P.get("DontCheckDefaultBrowser") is True


def test_first_run_and_bookmarks_neutral() -> None:
    assert P.get("OverrideFirstRunPage") == ""
    assert P.get("OverridePostUpdatePage") == ""
    assert P.get("NoDefaultBookmarks") is True


def test_in_app_updater_disabled() -> None:
    # ADR-0018: no OpenBook update server exists; the in-app updater would poll
    # Mozilla's endpoint (unsolicited egress) and could serve stock Firefox.
    # Updates flow via signed package channels (ADR-0013).
    assert P.get("DisableAppUpdate") is True


def test_tracking_protection_policy() -> None:
    etp = P.get("EnableTrackingProtection")
    assert isinstance(etp, dict)
    assert etp.get("Value") is True


def test_user_messaging_recommendations_off() -> None:
    um = P.get("UserMessaging")
    assert isinstance(um, dict)
    assert um.get("ExtensionRecommendations") is False
    assert um.get("FeatureRecommendations") is False
    assert um.get("WhatsNew") is False


def test_firefox_home_no_sponsored_or_pocket() -> None:
    fh = P.get("FirefoxHome")
    assert isinstance(fh, dict)
    assert fh.get("SponsoredTopSites") is False
    assert fh.get("Pocket") is False
    assert fh.get("SponsoredPocket") is False


def test_search_engine_privacy_default() -> None:
    se = P.get("SearchEngines")
    assert isinstance(se, dict)
    assert se.get("Default") == "DuckDuckGo"
    # Users may still install/change engines — not a lock.
    assert se.get("PreventInstalls") in (False, None)


def test_bundled_extensions_allowed() -> None:
    ext = P.get("ExtensionSettings")
    assert isinstance(ext, dict)
    for ext_id in (
        "vault-ui@openbook.browser",
        "proxy-manager@openbook.browser",
        "ai-sidebar@openbook.browser",
    ):
        assert ext_id in ext, f"missing ExtensionSettings entry for {ext_id}"
        assert ext[ext_id].get("installation_mode") == "allowed"
    # Sane default for everything else.
    assert "*" in ext
    assert ext["*"].get("installation_mode") in ("allowed", "blocked")


def test_dns_over_https_present() -> None:
    doh = P.get("DNSOverHTTPS")
    assert isinstance(doh, dict)
    assert doh.get("Enabled") is True


def main() -> None:
    fns = [v for k, v in sorted(globals().items())
           if k.startswith("test_") and callable(v)]
    for fn in fns:
        fn()
    print(f"OK: policies.json valid; {len(P)} policies; {len(fns)} checks.")


if __name__ == "__main__":
    main()
