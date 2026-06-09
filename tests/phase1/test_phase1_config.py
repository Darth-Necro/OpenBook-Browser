#!/usr/bin/env python3
"""Phase 1 structure and content validation for config, patches, and scripts."""
import json
import os
import stat
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

REQUIRED_FILES = [
    "config/autoconfig/autoconfig.js",
    "config/autoconfig/openbook.cfg",
    "config/distribution/policies.json",
    "patches/branding/0001-branding-add-openbook-brand-directory.patch",
    "build/scripts/install-config.sh",
    ".github/workflows/phase1.yml",
]

REQUIRED_DIRS = [
    "config/autoconfig",
    "config/distribution",
    "patches/branding",
    "tests/phase1",
]


def assert_file(relative: str) -> None:
    p = ROOT / relative
    assert p.is_file(), f"Missing file: {relative}"


def assert_dir(relative: str) -> None:
    p = ROOT / relative
    assert p.is_dir(), f"Missing directory: {relative}"


def assert_executable(relative: str) -> None:
    p = ROOT / relative
    mode = p.stat().st_mode
    assert mode & stat.S_IXUSR, f"Script not user-executable: {relative}"


def assert_contains(relative: str, text: str) -> None:
    content = (ROOT / relative).read_text(encoding="utf-8")
    assert text in content, f"{relative!r} does not contain {text!r}"


def main() -> None:
    for d in REQUIRED_DIRS:
        assert_dir(d)

    for f in REQUIRED_FILES:
        assert_file(f)

    # install-config.sh must be executable
    assert_executable("build/scripts/install-config.sh")

    # autoconfig.js must wire up the config filename
    assert_contains("config/autoconfig/autoconfig.js", "general.config.filename")
    assert_contains("config/autoconfig/autoconfig.js", "openbook.cfg")

    # openbook.cfg must start with a comment (Firefox AutoConfig requirement)
    cfg_text = (ROOT / "config/autoconfig/openbook.cfg").read_text(encoding="utf-8")
    assert cfg_text.startswith("//"), "openbook.cfg must start with a comment line"

    # openbook.cfg must lock telemetry prefs
    assert_contains("config/autoconfig/openbook.cfg", "toolkit.telemetry.enabled")
    assert_contains("config/autoconfig/openbook.cfg", "lockPref")

    # openbook.cfg must lock AI prefs (security invariant 5)
    assert_contains("config/autoconfig/openbook.cfg", "browser.ml.enable")

    # policies.json must be valid JSON with a top-level "policies" key
    policies_path = ROOT / "config/distribution/policies.json"
    policies = json.loads(policies_path.read_text(encoding="utf-8"))
    assert "policies" in policies, "policies.json must have a top-level 'policies' key"
    assert policies["policies"].get("DisableTelemetry") is True, (
        "policies.json must set DisableTelemetry: true"
    )

    # branding patch must introduce openbook brand directory
    assert_contains(
        "patches/branding/0001-branding-add-openbook-brand-directory.patch",
        "browser/branding/openbook",
    )
    assert_contains(
        "patches/branding/0001-branding-add-openbook-brand-directory.patch",
        "OpenBook Browser",
    )

    # install-config.sh must reference all three config artifacts
    assert_contains("build/scripts/install-config.sh", "autoconfig.js")
    assert_contains("build/scripts/install-config.sh", "openbook.cfg")
    assert_contains("build/scripts/install-config.sh", "policies.json")

    print("Phase 1 config checks passed.")


if __name__ == "__main__":
    main()
