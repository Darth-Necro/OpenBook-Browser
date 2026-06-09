#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — release permissions verification (Build Plan §11).
#
# Asserts the privileged files in a STAGED release tree are root-owned and not
# group/other-writable. A user-writable privileged AutoConfig or native-messaging
# host is a local privilege-escalation hole and is a release blocker. Run on the
# staged package root in release CI (Linux packaging host; uses GNU stat).
#
# Usage: verify-release-permissions.sh --root DIR
# Exit:  0 ok; 2 usage; 3 missing required file; 5 ownership/mode violation.
set -euo pipefail

PROG="$(basename "$0")"
ROOT=""

usage() {
  cat <<USAGE
$PROG --root DIR

  --root DIR   staged release tree to check (openbook.cfg,
               defaults/pref/autoconfig.js, distribution/policies.json, plus any
               native messaging host manifests/binaries present).
USAGE
}

require_value() {
  if [[ "$2" -lt 2 || "${3:-}" == --* ]]; then
    echo "Option $1 requires a value." >&2
    usage >&2
    exit 2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --root) require_value "$1" "$#" "${2:-}"; ROOT="$2"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ -n "$ROOT" ]] || { echo "--root is required." >&2; usage >&2; exit 2; }
[[ -d "$ROOT" ]] || { echo "root directory does not exist: $ROOT" >&2; exit 3; }

rc=0
check() {
  # check FILE REQUIRED(required|optional)
  local f="$1" required="$2" uid mode
  if [[ ! -e "$f" ]]; then
    if [[ "$required" == "required" ]]; then
      echo "$PROG: missing required privileged file: $f" >&2
      rc=3
    fi
    return
  fi
  uid="$(stat -c '%u' "$f")"
  mode="$(stat -c '%a' "$f")"
  if [[ "$uid" != "0" ]]; then echo "$PROG: NOT root-owned (uid=$uid): $f" >&2; rc=5; fi
  if (( 0$mode & 022 )); then echo "$PROG: group/other-writable (mode=$mode): $f" >&2; rc=5; fi
}

# Required Phase 1 hardening layer.
check "$ROOT/openbook.cfg" required
check "$ROOT/defaults/pref/autoconfig.js" required
check "$ROOT/distribution/policies.json" required

# Native messaging host manifests + binaries, if present in this tree.
while IFS= read -r f; do check "$f" optional; done < <(find "$ROOT" -type f -name 'org.openbook.*.json' 2>/dev/null)
while IFS= read -r f; do check "$f" optional; done < <(find "$ROOT" -type f \( -name 'openbook-vault-host' -o -name 'openbook-vpn-helper' \) 2>/dev/null)

if [[ "$rc" -eq 0 ]]; then
  echo "$PROG: OK — privileged files are root-owned and not user-writable in $ROOT"
fi
exit "$rc"
