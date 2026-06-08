#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR=""
TARGET="linux-x64"
ARTIFACT=0
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

usage() {
  cat <<USAGE
Usage: $0 --source DIR --target linux-x64|win-x64|macos-universal [--artifact]

Invokes Firefox's mach build with the matching Phase 0 mozconfig.
USAGE
}

require_value() {
  # require_value FLAG REMAINING NEXT — fail with usage if no value follows FLAG.
  if [[ "$2" -lt 2 || "${3:-}" == --* ]]; then
    echo "Option $1 requires a value." >&2
    usage >&2
    exit 2
  fi
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      require_value "$1" "$#" "${2:-}"
      SOURCE_DIR="$2"
      shift 2
      ;;
    --target)
      require_value "$1" "$#" "${2:-}"
      TARGET="$2"
      shift 2
      ;;
    --artifact)
      ARTIFACT=1
      shift
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

if [[ -z "$SOURCE_DIR" || ! -d "$SOURCE_DIR" ]]; then
  echo "A valid --source directory is required." >&2
  exit 3
fi
if [[ ! -x "$SOURCE_DIR/mach" ]]; then
  echo "Firefox mach executable not found in source directory: $SOURCE_DIR" >&2
  exit 3
fi

case "$TARGET" in
  linux-x64|win-x64|macos-universal)
    MOZCONFIG_PATH="$REPO_ROOT/build/mozconfig/mozconfig.${TARGET}"
    ;;
  *)
    echo "Unsupported target: $TARGET" >&2
    exit 2
    ;;
esac

if [[ "$ARTIFACT" -eq 1 ]]; then
  MOZCONFIG_PATH="$REPO_ROOT/build/mozconfig/mozconfig.artifact"
fi
if [[ ! -r "$MOZCONFIG_PATH" ]]; then
  echo "Mozconfig not readable: $MOZCONFIG_PATH" >&2
  exit 3
fi

cd "$SOURCE_DIR"
export MOZCONFIG="$MOZCONFIG_PATH"
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1735689600}"

echo "Building target ${TARGET} with MOZCONFIG=${MOZCONFIG}."
./mach build
