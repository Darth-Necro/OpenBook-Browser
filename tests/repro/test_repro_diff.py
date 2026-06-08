#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
#
# Tests for repro_diff.py (Build Plan §5). Runnable now:
#   python3 tests/repro/test_repro_diff.py    -> exits 0 on success
# Also pytest-discoverable (test_* functions).
#
# Coverage:
#   - two byte-identical trees report MATCH (return code 0),
#   - mutating one file's CONTENT reports MISMATCH (nonzero),
#   - adding an EXTRA file reports MISMATCH,
#   - REMOVING a file reports MISMATCH,
#   - traversal order / different root locations do NOT affect the result,
#   - the CLI returns the documented exit codes (0 match, 1 mismatch, 2 usage).

import os
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPRO_DIFF = os.path.join(HERE, "repro_diff.py")

# Import the module under test directly for in-process assertions.
sys.path.insert(0, HERE)
import repro_diff  # noqa: E402


def _make_tree(root, files):
    """Create `files` (dict relpath -> bytes) under `root`, making dirs as needed."""
    for rel, data in files.items():
        full = os.path.join(root, rel)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "wb") as fh:
            fh.write(data)


# Canonical fixture: a small, nested, deterministic tree.
FIXTURE = {
    "firefox/openbook": b"\x7fELF fake binary bytes",
    "firefox/libxul.so": b"x" * 4096,
    "firefox/defaults/pref/autoconfig.js": b'pref("general.config.filename","openbook.cfg");\n',
    "firefox/openbook.cfg": b"// comment first line\nlockPref(\"x\", true);\n",
    "firefox/browser/omni.ja": bytes(range(256)),
}


def _check(cond, msg, failures):
    if cond:
        print(f"  PASS: {msg}")
    else:
        print(f"  FAIL: {msg}")
        failures.append(msg)


def _run() -> int:
    failures = []

    with tempfile.TemporaryDirectory() as tmp:
        a = os.path.join(tmp, "rebuild")
        b = os.path.join(tmp, "published")
        os.makedirs(a)
        os.makedirs(b)
        _make_tree(a, FIXTURE)
        _make_tree(b, FIXTURE)

        # 1. Byte-identical trees -> MATCH.
        matched, diffs = repro_diff.compare(a, b)
        _check(matched and not diffs, "byte-identical trees report MATCH", failures)

        # CLI agrees: exit code 0 and prints MATCH.
        cp = subprocess.run(
            [sys.executable, REPRO_DIFF, a, b], capture_output=True, text=True
        )
        _check(cp.returncode == 0, "CLI returns 0 for identical trees", failures)
        _check("MATCH" in cp.stdout, "CLI prints MATCH for identical trees", failures)

        # 2. Mutate one file's content -> MISMATCH (nonzero).
        with open(os.path.join(b, "firefox/libxul.so"), "wb") as fh:
            fh.write(b"x" * 4095 + b"Y")  # one byte differs, same length
        matched, diffs = repro_diff.compare(a, b)
        _check(not matched and diffs, "single-byte content change reports MISMATCH", failures)
        _check(
            any("libxul.so" in d for d in diffs),
            "MISMATCH names the differing file (libxul.so)",
            failures,
        )

        cp = subprocess.run(
            [sys.executable, REPRO_DIFF, a, b], capture_output=True, text=True
        )
        _check(cp.returncode == 1, "CLI returns 1 (nonzero) on content mismatch", failures)
        _check("MISMATCH" in cp.stdout, "CLI prints MISMATCH on content change", failures)

        # restore b to identical for the next sub-case
        _make_tree(b, {"firefox/libxul.so": FIXTURE["firefox/libxul.so"]})
        matched, _ = repro_diff.compare(a, b)
        _check(matched, "restoring the file returns trees to MATCH", failures)

        # 3. Extra file in B -> MISMATCH.
        _make_tree(b, {"firefox/extra.txt": b"surprise"})
        matched, diffs = repro_diff.compare(a, b)
        _check(
            not matched and any("only in B" in d and "extra.txt" in d for d in diffs),
            "an extra file in B reports MISMATCH ('only in B')",
            failures,
        )
        os.remove(os.path.join(b, "firefox/extra.txt"))

        # 4. Missing file in B -> MISMATCH.
        os.remove(os.path.join(b, "firefox/openbook.cfg"))
        matched, diffs = repro_diff.compare(a, b)
        _check(
            not matched and any("only in A" in d and "openbook.cfg" in d for d in diffs),
            "a removed file reports MISMATCH ('only in A')",
            failures,
        )

    # 5. Normalization: trees at different absolute roots, created in a different
    #    order, still MATCH (manifest is sorted + relative).
    with tempfile.TemporaryDirectory() as t1, tempfile.TemporaryDirectory() as t2:
        forward = dict(FIXTURE)
        reversed_items = dict(reversed(list(FIXTURE.items())))
        _make_tree(t1, forward)
        _make_tree(t2, reversed_items)
        matched, diffs = repro_diff.compare(t1, t2)
        _check(
            matched and not diffs,
            "different root locations + creation order still MATCH (normalized)",
            failures,
        )

    # 6. Single-file comparison + usage error exit code.
    with tempfile.TemporaryDirectory() as tmp:
        f1 = os.path.join(tmp, "one.bin")
        f2 = os.path.join(tmp, "two.bin")
        with open(f1, "wb") as fh:
            fh.write(b"same bytes")
        with open(f2, "wb") as fh:
            fh.write(b"same bytes")
        matched, _ = repro_diff.compare(f1, f2)
        _check(matched, "two identical lone files MATCH (keyed by basename-agnostic bytes)", failures)

        # Nonexistent path -> usage/input error exit code 2.
        cp = subprocess.run(
            [sys.executable, REPRO_DIFF, f1, os.path.join(tmp, "missing.bin")],
            capture_output=True,
            text=True,
        )
        _check(cp.returncode == 2, "CLI returns 2 on a nonexistent input path", failures)

    print()
    if failures:
        print(f"REPRO DIFF TESTS: {len(failures)} assertion(s) failed.")
        return 1
    print("REPRO DIFF TESTS: all assertions passed.")
    return 0


# --- pytest entry points -----------------------------------------------------


def test_repro_diff_match_and_mismatch():
    assert _run() == 0


if __name__ == "__main__":
    sys.exit(_run())
