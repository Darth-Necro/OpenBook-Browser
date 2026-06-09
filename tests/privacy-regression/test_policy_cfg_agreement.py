#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""openbook.cfg (AutoConfig) and policies.json (enterprise policy) are
defense-in-depth duplicates and must agree on the core privacy locks: if one
disables telemetry/studies/Pocket, so must the other."""
import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CFG = (ROOT / "config/autoconfig/openbook.cfg").read_text(encoding="utf-8")
POL = json.loads((ROOT / "config/policies/policies.json").read_text(encoding="utf-8"))
POLICIES = POL.get("policies", POL)


def cfg_sets(pref: str, value: str) -> bool:
    pat = re.compile(
        r'(?:lockPref|defaultPref|pref)\(\s*"' + re.escape(pref) + r'"\s*,\s*' + re.escape(value) + r'\s*\)'
    )
    return bool(pat.search(CFG))


def test_telemetry_agree() -> None:
    assert cfg_sets("toolkit.telemetry.enabled", "false"), "cfg does not disable telemetry"
    assert POLICIES.get("DisableTelemetry") is True, "policies.json does not DisableTelemetry"


def test_studies_agree() -> None:
    assert cfg_sets("app.shield.optoutstudies.enabled", "false"), "cfg does not disable studies"
    assert POLICIES.get("DisableFirefoxStudies") is True, "policies.json does not DisableFirefoxStudies"


def test_pocket_agree() -> None:
    assert cfg_sets("extensions.pocket.enabled", "false"), "cfg does not disable Pocket"
    assert POLICIES.get("DisablePocket") is True, "policies.json does not DisablePocket"


if __name__ == "__main__":
    test_telemetry_agree()
    test_studies_agree()
    test_pocket_agree()
    print("OK policy/cfg agreement")
