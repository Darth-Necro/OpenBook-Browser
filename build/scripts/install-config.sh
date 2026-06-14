#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — install the AutoConfig + enterprise-policy settings layer
# into a built Firefox dist (Phase 1; Build Plan §4/§11).
#
# Installs, with mode 0644:
#   <dist>/defaults/pref/autoconfig.js   loads openbook.cfg
#   <dist>/openbook.cfg                  privileged AutoConfig hardening
#   <dist>/distribution/policies.json    enterprise policy (defense in depth)
#
# Fails closed: aborts nonzero if a source file is missing or a copy fails.
#   --verify        do not install; assert the config files are present (used by
#                   package.sh before packaging).
#   --require-root  also assert installed/verified files are root-owned and not
#                   group/other-writable (release invariant §11; GNU stat, Linux).
#
# Exit: 0 ok; 2 usage; 3 missing source/dist; 5 permission check failed.
set -euo pipefail

PROG="$(basename "$0")"
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

DIST=""
MODE="install"   # install | verify
REQUIRE_ROOT=0

usage() {
  cat <<USAGE
$PROG --dist DIR [--verify] [--require-root]

  --dist DIR       dist dir to install into / verify (e.g. <objdir>/dist/bin on
                   Linux/Windows, or the macOS .app/Contents/Resources).
  --verify         do not install; assert the OpenBook config files exist.
  --require-root   also assert files are root-owned, not group/other-writable.
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
    --dist) require_value "$1" "$#" "${2:-}"; DIST="$2"; shift 2 ;;
    --verify) MODE="verify"; shift ;;
    --require-root) REQUIRE_ROOT=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; usage >&2; exit 2 ;;
  esac
done

[[ -n "$DIST" ]] || { echo "--dist is required." >&2; usage >&2; exit 2; }
[[ -d "$DIST" ]] || { echo "dist directory does not exist: $DIST" >&2; exit 3; }
# Canonicalize so the §11 dir-chain walk's stop condition (d == DIST) matches
# dirname's normalized output: a trailing slash (e.g. --dist /opt/openbook/) or
# a symlinked component would otherwise send the walk above DIST and spuriously
# fail an otherwise-correct install.
DIST="$(realpath -- "$DIST")" || { echo "cannot resolve --dist: $DIST" >&2; exit 3; }

src_autoconfig="$REPO_ROOT/config/autoconfig/autoconfig.js"
src_cfg="$REPO_ROOT/config/autoconfig/openbook.cfg"
src_policies="$REPO_ROOT/config/policies/policies.json"

dst_autoconfig="$DIST/defaults/pref/autoconfig.js"
dst_cfg="$DIST/openbook.cfg"
dst_policies="$DIST/distribution/policies.json"

check_perms() {
  # check_perms FILE -> assert root-owned and not group/other writable (§11),
  # INCLUDING the directory chain up to $DIST: a locked-down file inside a
  # user-writable directory is still replaceable (rename + recreate), so the
  # invariant only holds when the whole containing chain is locked down.
  local f="$1" uid mode d
  uid="$(stat -c '%u' "$f")"
  mode="$(stat -c '%a' "$f")"
  if [[ "$uid" != "0" ]]; then
    echo "$PROG: $f not root-owned (uid=$uid); §11 requires root ownership in releases." >&2
    return 1
  fi
  if (( 0$mode & 022 )); then
    echo "$PROG: $f is group/other-writable (mode=$mode); §11 forbids this." >&2
    return 1
  fi
  d="$(dirname -- "$f")"
  while :; do
    uid="$(stat -c '%u' "$d")"
    mode="$(stat -c '%a' "$d")"
    if [[ "$uid" != "0" ]]; then
      echo "$PROG: parent dir of $f not root-owned (uid=$uid): $d (§11)." >&2
      return 1
    fi
    if (( 0$mode & 022 )); then
      echo "$PROG: parent dir of $f is group/other-writable (mode=$mode): $d (§11)." >&2
      return 1
    fi
    [[ "$d" == "$DIST" || "$d" == "/" || "$d" == "." ]] && break
    d="$(dirname -- "$d")"
  done
  return 0
}

if [[ "$MODE" == "verify" ]]; then
  rc=0
  for f in "$dst_autoconfig" "$dst_cfg" "$dst_policies"; do
    if [[ ! -f "$f" ]]; then echo "$PROG: missing required config: $f" >&2; rc=1; continue; fi
    if [[ "$REQUIRE_ROOT" -eq 1 ]]; then check_perms "$f" || rc=1; fi
  done
  [[ "$rc" -eq 0 ]] && echo "$PROG: verified OpenBook config present in $DIST"
  exit "$rc"
fi

for f in "$src_autoconfig" "$src_cfg" "$src_policies"; do
  [[ -f "$f" ]] || { echo "$PROG: source config missing: $f" >&2; exit 3; }
done

install_file() {
  local src="$1" dst="$2"
  mkdir -p "$(dirname "$dst")"
  cp "$src" "$dst"
  chmod 0644 "$dst"
  echo "$PROG: installed $dst"
}

install_file "$src_autoconfig" "$dst_autoconfig"
install_file "$src_cfg" "$dst_cfg"
install_file "$src_policies" "$dst_policies"

if [[ "$REQUIRE_ROOT" -eq 1 ]]; then
  rc=0
  for f in "$dst_autoconfig" "$dst_cfg" "$dst_policies"; do check_perms "$f" || rc=1; done
  [[ "$rc" -eq 0 ]] || { echo "$PROG: post-install permission check failed (§11)." >&2; exit 5; }
fi

echo "$PROG: OpenBook AutoConfig + policies installed into $DIST"
