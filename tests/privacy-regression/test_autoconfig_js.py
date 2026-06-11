#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
"""Privacy-regression: assert config/autoconfig/autoconfig.js wires up the
external AutoConfig file correctly.

autoconfig.js ships to defaults/pref/ and must point Firefox at openbook.cfg
with a non-obscured (plaintext, auditable) value, and must NOT name a different
config file. Pure stdlib; runnable with python3 and pytest-discoverable.
"""
from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
JS = ROOT / "config" / "autoconfig" / "autoconfig.js"

_PREF = re.compile(
    r"""pref\s*\(\s*(["'])(?P<name>[^"']+)\1\s*,\s*(?P<value>.+?)\s*\)\s*;"""
)


def _prefs() -> dict[str, str]:
    out: dict[str, str] = {}
    for raw in JS.read_text(encoding="utf-8").splitlines():
        line = raw.split("//", 1)[0]
        m = _PREF.search(line)
        if m:
            out[m.group("name")] = m.group("value").strip()
    return out


PREFS = _prefs()


def test_file_exists() -> None:
    assert JS.is_file(), f"missing {JS}"


def test_config_filename_is_openbook_cfg() -> None:
    assert PREFS.get("general.config.filename") in (
        '"openbook.cfg"',
        "'openbook.cfg'",
    ), f"general.config.filename must be openbook.cfg, got {PREFS.get('general.config.filename')!r}"


def test_obscure_value_zero() -> None:
    # 0 => plaintext .cfg (auditable). Obscuring is not a security control.
    assert PREFS.get("general.config.obscure_value") == "0", (
        "general.config.obscure_value must be 0 (plaintext, auditable)"
    )


def test_sandbox_not_disabled() -> None:
    # openbook.cfg uses only defaultPref()/lockPref(), which the AutoConfig
    # sandbox provides. Disabling the sandbox would grant the .cfg full chrome
    # privilege for no functional gain (least privilege, §11).
    assert PREFS.get("general.config.sandbox_enabled") != "false", (
        "do not disable the AutoConfig sandbox; the cfg only needs the sandboxed API"
    )


def test_spdx_header_present() -> None:
    head = JS.read_text(encoding="utf-8").splitlines()[0]
    assert "SPDX-License-Identifier: MPL-2.0" in head


def main() -> None:
    fns = [v for k, v in sorted(globals().items())
           if k.startswith("test_") and callable(v)]
    for fn in fns:
        fn()
    print(f"OK: autoconfig.js verified ({len(fns)} checks).")


if __name__ == "__main__":
    main()
