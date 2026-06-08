#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""The branding the build selects must actually exist and be staged into the tree.

The branding patch defaults MOZ_BRANDING_DIRECTORY to browser/branding/openbook
and the mozconfigs select it; this asserts the assets exist under branding/openbook
and that build.sh stages them into the source tree (so the selection resolves to a
populated directory rather than a missing one)."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BRAND = ROOT / "branding" / "openbook"
BUILD = (ROOT / "build/scripts/build.sh").read_text(encoding="utf-8")

REQUIRED_BRANDING = [
    "configure.sh",
    "locales/en-US/brand.properties",
    "locales/en-US/brand.ftl",
]


def test_branding_assets_present() -> None:
    assert BRAND.is_dir(), "branding/openbook missing"
    for rel in REQUIRED_BRANDING:
        assert (BRAND / rel).is_file(), f"branding asset missing: {rel}"
    assert any((BRAND / "content").glob("*.svg")), "no branding content SVG assets"


def test_build_stages_branding_into_source_tree() -> None:
    assert "branding/openbook" in BUILD, "build.sh does not reference the branding source"
    assert "browser/branding/openbook" in BUILD, "build.sh does not stage into browser/branding/openbook"


if __name__ == "__main__":
    test_branding_assets_present()
    test_build_stages_branding_into_source_tree()
    print("OK branding assets + staging")
