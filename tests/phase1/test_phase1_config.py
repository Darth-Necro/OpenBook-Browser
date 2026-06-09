#!/usr/bin/env python3
"""Phase 1 structure and content validation for config, patches, and scripts."""
import json
import stat
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

REQUIRED_FILES = [
    "config/autoconfig/autoconfig.js",
    "config/autoconfig/openbook.cfg",
    "config/distribution/policies.json",
    "patches/branding/0001-branding-add-openbook-brand-directory.patch",
    "patches/SERIES",
    "build/scripts/install-config.sh",
    ".github/workflows/phase1.yml",
    ".github/dependabot.yml",
    "tests/leak/test_first_run_egress.py",
    "branding/openbook/default-prefs.js",
    "branding/openbook/content/about-logo.png",
    "branding/openbook/content/about-logo@2x.png",
    "branding/openbook/content/about-logo.svg",
    "branding/openbook/content/identity-icons-brand.svg",
    "branding/openbook/content/favicon32.ico",
    "branding/openbook/content/favicon64.ico",
]

REQUIRED_DIRS = [
    "config/autoconfig",
    "config/distribution",
    "patches/branding",
    "branding/openbook/content",
    "tests/phase1",
    "tests/leak",
]

# LibreWolf-equivalent baseline locks that must be present in openbook.cfg.
REQUIRED_CFG_LOCKS = [
    # NOTE: OpenBook KEEPS Safe Browsing ENABLED (openbook.cfg §4 — malware/
    # phishing protection is a net safety win), unlike a stock LibreWolf
    # baseline, so the provider updateURL prefs are intentionally left intact
    # and not asserted blank here.
    "browser.safebrowsing.malware.enabled",
    "browser.safebrowsing.phishing.enabled",
    "network.dns.disablePrefetch",
    "network.predictor.enabled",
    "network.http.speculative-parallel-limit",
    "geo.enabled",
    "browser.send_pings",
    "extensions.systemAddon.update.enabled",
]

# Compile-time hardening that must appear in every platform mozconfig.
REQUIRED_MOZCONFIG_FLAGS = [
    "--disable-crashreporter",
    "--disable-updater",
    "--disable-eme",
    "MOZ_TELEMETRY_REPORTING=",
    "MOZ_NORMANDY=",
]

PLATFORM_MOZCONFIGS = [
    "build/mozconfig/mozconfig.linux-x64",
    "build/mozconfig/mozconfig.win-x64",
    "build/mozconfig/mozconfig.macos-universal",
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

    # openbook.cfg must include the LibreWolf-equivalent baseline.
    for pref in REQUIRED_CFG_LOCKS:
        assert_contains("config/autoconfig/openbook.cfg", pref)

    # Every platform mozconfig must compile out telemetry/crashreporter/updater/EME.
    for mozconfig in PLATFORM_MOZCONFIGS:
        for flag in REQUIRED_MOZCONFIG_FLAGS:
            assert_contains(mozconfig, flag)

    # install-config.sh must use read-only install mode for privileged files.
    assert_contains("build/scripts/install-config.sh", "install -m 0444")

    # apply-patches.sh must apply hunks with --fuzz=0 (exact context): a privacy/
    # security hunk applying at drifted context is a silent semantic change, so
    # the series demands exact line matches or fails hard.
    apply_text = (ROOT / "build/scripts/apply-patches.sh").read_text(encoding="utf-8")
    assert "--fuzz=0" in apply_text, (
        "apply-patches.sh must invoke patch(1) with --fuzz=0 (exact context)"
    )
    # SERIES manifest must exist and be referenced by apply-patches.sh.
    assert "SERIES" in apply_text, "apply-patches.sh must consult patches/SERIES"

    # fetch-verify-upstream.sh must pin a signer fingerprint.
    assert_contains("build/scripts/fetch-verify-upstream.sh", "EXPECTED_KEY_FPRS")
    assert_contains("build/scripts/fetch-verify-upstream.sh", "VALIDSIG")

    # Workflows must declare read-only default permissions.
    assert_contains(".github/workflows/phase0.yml", "permissions:")
    assert_contains(".github/workflows/phase1.yml", "permissions:")

    # policies.json must be valid JSON with a top-level "policies" key
    policies_path = ROOT / "config/distribution/policies.json"
    policies = json.loads(policies_path.read_text(encoding="utf-8"))
    assert "policies" in policies, "policies.json must have a top-level 'policies' key"
    assert policies["policies"].get("DisableTelemetry") is True, (
        "policies.json must set DisableTelemetry: true"
    )
    # No-op SanitizeOnShutdown block must not be reintroduced.
    sos = policies["policies"].get("SanitizeOnShutdown")
    if sos is not None:
        assert any(v is True for k, v in sos.items() if k != "Locked"), (
            "policies.json SanitizeOnShutdown must sanitize at least one category"
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
