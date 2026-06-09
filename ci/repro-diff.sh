#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — reproducible-build diff (CI wrapper, Build Plan §5/§10).
#
# Thin wrapper around tests/repro/repro_diff.py so CI and users have a stable
# `ci/` entry point. All arguments are passed straight through.
#
#   ci/repro-diff.sh REBUILD_DIR PUBLISHED_DIR
#   ci/repro-diff.sh rebuilt.tar.xz published.tar.xz
#   ci/repro-diff.sh --manifest-only DIR
#
# Exit code is repro_diff.py's: 0 MATCH, 1 MISMATCH, 2 usage/input error.

set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." >/dev/null 2>&1 && pwd)"
REPRO_DIFF="$REPO_ROOT/tests/repro/repro_diff.py"

if ! command -v python3 >/dev/null 2>&1; then
  echo "ci/repro-diff.sh: error: python3 not found" >&2
  exit 2
fi

if [[ ! -f "$REPRO_DIFF" ]]; then
  echo "ci/repro-diff.sh: error: missing $REPRO_DIFF" >&2
  exit 2
fi

exec python3 "$REPRO_DIFF" "$@"
