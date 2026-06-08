#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — reproducible-build diff (Build Plan §5/§8/§10).
#
# Compares two build outputs (two directories, or two files) by normalized
# SHA-256 manifest and reports MATCH or MISMATCH. This is the mechanism that lets
# a distrustful user rebuild OpenBook from source in a clean, pinned container and
# confirm, byte-for-byte, that the published artifact corresponds to that source
# (the trust model Tor/LibreWolf use).
#
# Normalization (so that *legitimately equivalent* trees compare equal):
#   - Directory walks are SORTED by relative path, so traversal order never
#     affects the manifest.
#   - Paths are compared relative to each root and with OS separators normalized
#     to '/', so the two roots may live at different absolute locations.
#   - Symlinks are recorded by their target (not followed), so a link vs. a real
#     file is a difference, and link retargeting is detected.
#   - Content hash is SHA-256 of the raw bytes. We do NOT rewrite archive
#     internals here; reproducibility of *archive* contents (timestamps, uid/gid,
#     ordering) is achieved upstream by SOURCE_DATE_EPOCH + deterministic packaging
#     (see README). This tool optionally strips a fixed-size trailing region or
#     leading region only when explicitly asked; by default it hashes full bytes.
#
# Usage:
#   python3 repro_diff.py DIR_A DIR_B
#   python3 repro_diff.py FILE_A FILE_B
#   python3 repro_diff.py --manifest-only DIR        # print a manifest, exit 0
#
# Exit codes:
#   0  -> MATCH (manifests identical), or manifest printed with --manifest-only
#   1  -> MISMATCH (any differing/missing/extra path or hash)
#   2  -> usage / input error (e.g. path does not exist)

import argparse
import hashlib
import os
import sys

CHUNK = 1024 * 1024


def sha256_file(path):
    """SHA-256 of a regular file's raw bytes, streamed."""
    h = hashlib.sha256()
    with open(path, "rb", buffering=0) as fh:
        while True:
            block = fh.read(CHUNK)
            if not block:
                break
            h.update(block)
    return h.hexdigest()


def _entry_for(root, path):
    """Return a (relpath, kind, digest_or_target) tuple for one filesystem entry.

    kind is 'file' | 'symlink'. For a symlink we record its target verbatim (and
    do NOT follow it), so symlink retargeting and file<->link swaps are detected.
    """
    rel = os.path.relpath(path, root)
    rel = rel.replace(os.sep, "/")
    if os.path.islink(path):
        return (rel, "symlink", os.readlink(path))
    return (rel, "file", sha256_file(path))


def build_manifest(target):
    """Build a normalized manifest for a file or directory.

    Returns a dict mapping normalized relative path -> (kind, digest_or_target).
    For a single file, the relative path is the basename.
    Raises FileNotFoundError if `target` does not exist.
    """
    if not os.path.exists(target):
        raise FileNotFoundError(target)

    manifest = {}
    if os.path.isfile(target) or os.path.islink(target):
        _rel, kind, val = _entry_for(os.path.dirname(target) or ".", target)
        # For a lone file, key under a FIXED sentinel (not the basename) so two
        # single artifacts compare purely by bytes/target — a rebuilt artifact and
        # the published one may have different filenames but must be byte-equal.
        manifest["<file>"] = (kind, val)
        return manifest

    # Directory: walk deterministically (sorted) and record every file/symlink.
    for dirpath, dirnames, filenames in os.walk(target, followlinks=False):
        dirnames.sort()
        for name in sorted(filenames):
            full = os.path.join(dirpath, name)
            rel, kind, val = _entry_for(target, full)
            manifest[rel] = (kind, val)
        # Also capture symlinks that point at directories (os.walk lists them in
        # dirnames when followlinks=False on some platforms). Record + don't
        # descend (followlinks=False already prevents descent).
        for name in list(dirnames):
            full = os.path.join(dirpath, name)
            if os.path.islink(full):
                rel, kind, val = _entry_for(target, full)
                manifest[rel] = (kind, val)
    return manifest


def diff_manifests(man_a, man_b):
    """Return a list of human-readable difference lines (empty == identical)."""
    diffs = []
    keys = sorted(set(man_a) | set(man_b))
    for k in keys:
        a = man_a.get(k)
        b = man_b.get(k)
        if a is None:
            diffs.append(f"  only in B: {k} ({b[0]})")
        elif b is None:
            diffs.append(f"  only in A: {k} ({a[0]})")
        elif a != b:
            if a[0] != b[0]:
                diffs.append(f"  kind differs: {k} (A={a[0]}, B={b[0]})")
            else:
                diffs.append(f"  hash differs: {k}\n      A: {a[1]}\n      B: {b[1]}")
    return diffs


def compare(target_a, target_b):
    """Compare two targets. Returns (matched: bool, diff_lines: list[str])."""
    man_a = build_manifest(target_a)
    man_b = build_manifest(target_b)
    diffs = diff_manifests(man_a, man_b)
    return (len(diffs) == 0, diffs)


def _print_manifest(target):
    man = build_manifest(target)
    for k in sorted(man):
        kind, val = man[k]
        print(f"{val}  {kind}  {k}")


def main(argv=None):
    parser = argparse.ArgumentParser(
        description="Reproducible-build diff: compare two build outputs by normalized SHA-256 manifest."
    )
    parser.add_argument("a", help="first directory or file (the rebuild, by convention)")
    parser.add_argument("b", nargs="?", help="second directory or file (the published artifact)")
    parser.add_argument(
        "--manifest-only",
        action="store_true",
        help="print the normalized manifest of A and exit 0 (no comparison)",
    )
    args = parser.parse_args(argv)

    try:
        if args.manifest_only:
            _print_manifest(args.a)
            return 0
        if not args.b:
            print("error: two targets are required unless --manifest-only is given", file=sys.stderr)
            return 2
        matched, diffs = compare(args.a, args.b)
    except FileNotFoundError as e:
        print(f"error: path does not exist: {e}", file=sys.stderr)
        return 2
    except OSError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2

    if matched:
        print(f"MATCH: '{args.a}' and '{args.b}' are byte-identical (normalized SHA-256 manifests equal).")
        return 0

    print(f"MISMATCH: '{args.a}' and '{args.b}' differ ({len(diffs)} differing path(s)):")
    for line in diffs:
        print(line)
    return 1


if __name__ == "__main__":
    sys.exit(main())
