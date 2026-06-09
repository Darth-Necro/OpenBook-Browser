#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Every OpenBook mozconfig must select the OpenBook branding directory, so a
build never silently falls back to Firefox-trademarked branding (Build Plan §13)."""
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MOZ = ROOT / "build" / "mozconfig"
BRANDING = "--with-branding=browser/branding/openbook"


def test_all_mozconfigs_select_openbook_branding() -> None:
    files = sorted(MOZ.glob("mozconfig.*"))
    assert files, "no mozconfig files found"
    for f in files:
        text = f.read_text(encoding="utf-8")
        assert BRANDING in text, f"{f.name} does not select OpenBook branding ({BRANDING})"


if __name__ == "__main__":
    test_all_mozconfigs_select_openbook_branding()
    print("OK mozconfig branding")
