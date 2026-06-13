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


def test_release_mozconfigs_compile_out_egress_components() -> None:
    # ADR-0018 / invariant 1: release builds compile out the crash-report
    # uploader and the in-app updater. The artifact mozconfig is exempt (it
    # reuses Mozilla's prebuilt internals and must never be released) but must
    # say so explicitly.
    for name in ("mozconfig.linux-x64", "mozconfig.win-x64", "mozconfig.macos-universal"):
        text = (MOZ / name).read_text(encoding="utf-8")
        for flag in ("--disable-crashreporter", "--disable-updater"):
            assert flag in text, f"{name} missing {flag}"
    win = (MOZ / "mozconfig.win-x64").read_text(encoding="utf-8")
    assert "--disable-default-browser-agent" in win
    artifact = (MOZ / "mozconfig.artifact").read_text(encoding="utf-8")
    assert "never be released" in artifact, (
        "mozconfig.artifact must document why it lacks the privacy compile flags"
    )


if __name__ == "__main__":
    test_all_mozconfigs_select_openbook_branding()
    test_release_mozconfigs_compile_out_egress_components()
    print("OK mozconfig branding")
