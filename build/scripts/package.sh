#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      SOURCE_DIR="$2"
      shift 2
      ;;
    -h|--help)
      echo "Usage: $0 --source DIR"
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

if [[ -z "$SOURCE_DIR" || ! -x "$SOURCE_DIR/mach" ]]; then
  echo "A Firefox source directory with mach is required." >&2
  exit 3
fi

cd "$SOURCE_DIR"
./mach package
