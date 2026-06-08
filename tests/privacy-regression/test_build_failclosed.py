#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""The OpenBook hardening layer must be wired into the build and fail closed.

A build that "succeeds" without installing openbook.cfg / autoconfig.js /
policies.json would ship a browser with no telemetry-off and no enterprise
policies — a release-blocking security regression. These checks assert build.sh
installs the config and fails closed, and package.sh refuses to package a dist
that is missing it."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BUILD = (ROOT / "build/scripts/build.sh").read_text(encoding="utf-8")
PKG = (ROOT / "build/scripts/package.sh").read_text(encoding="utf-8")
INSTALL = ROOT / "build/scripts/install-config.sh"


def test_install_config_script_exists() -> None:
    assert INSTALL.is_file(), "build/scripts/install-config.sh missing"


def test_build_installs_config_and_fails_closed() -> None:
    assert "install-config.sh" in BUILD, "build.sh does not install the OpenBook config"
    assert "exit 4" in BUILD, "build.sh has no fail-closed exit when the dist/config is missing"
    # an explicit, clearly dev-only escape hatch — not a silent skip
    assert "--skip-config-install" in BUILD, "no explicit skip flag"
    assert "MUST NOT be released" in BUILD, "skip path is not labeled dev-only / unreleasable"
    # branding is staged into the source tree
    assert "browser/branding/openbook" in BUILD, "build.sh does not stage OpenBook branding"


def test_package_verifies_config_present() -> None:
    for token in ("autoconfig.js", "openbook.cfg", "policies.json"):
        assert token in PKG, f"package.sh does not verify {token} before packaging"
    assert ("die 7" in PKG) or ("exit 7" in PKG), "package.sh does not fail closed on missing config"


if __name__ == "__main__":
    test_install_config_script_exists()
    test_build_installs_config_and_fails_closed()
    test_package_verifies_config_present()
    print("OK build/package fail-closed wiring")
