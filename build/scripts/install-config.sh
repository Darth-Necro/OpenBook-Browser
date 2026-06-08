#!/usr/bin/env bash
set -euo pipefail

# Installs AutoConfig and enterprise policy files into a Firefox dist/bin directory.
# Call this after mach build; the dist directory is SOURCE_DIR/obj-openbook-TARGET/dist/bin.

DIST_DIR=""
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

usage() {
  cat <<USAGE
Usage: $0 --dist DIR

Copies config/autoconfig/ and config/distribution/ into a Firefox dist directory.

  --dist DIR   Path to the Firefox dist/bin (or equivalent) directory.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dist)
      DIST_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$DIST_DIR" ]]; then
  echo "--dist is required." >&2
  usage >&2
  exit 2
fi
if [[ ! -d "$DIST_DIR" ]]; then
  echo "Dist directory does not exist: $DIST_DIR" >&2
  exit 3
fi

# AutoConfig loader → defaults/pref/autoconfig.js
mkdir -p "$DIST_DIR/defaults/pref"
install -m 644 "$REPO_ROOT/config/autoconfig/autoconfig.js" "$DIST_DIR/defaults/pref/autoconfig.js"

# Locked config → install root (openbook.cfg)
install -m 644 "$REPO_ROOT/config/autoconfig/openbook.cfg" "$DIST_DIR/openbook.cfg"

# Enterprise policies → distribution/policies.json
mkdir -p "$DIST_DIR/distribution"
install -m 644 "$REPO_ROOT/config/distribution/policies.json" "$DIST_DIR/distribution/policies.json"

echo "OpenBook config files installed into ${DIST_DIR}."
