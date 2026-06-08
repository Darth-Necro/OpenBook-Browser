#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR=""
PATCH_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/patches"

usage() {
  cat <<USAGE
Usage: $0 --source DIR [--patch-root DIR]

Applies ordered patches from branding, privacy, and features subdirectories.
USAGE
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      SOURCE_DIR="$2"
      shift 2
      ;;
    --patch-root)
      PATCH_ROOT="$2"
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

if [[ -z "$SOURCE_DIR" ]]; then
  echo "--source is required." >&2
  usage >&2
  exit 2
fi
if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "Source directory does not exist: $SOURCE_DIR" >&2
  exit 3
fi
if [[ ! -d "$PATCH_ROOT" ]]; then
  echo "Patch root does not exist: $PATCH_ROOT" >&2
  exit 3
fi

mapfile -t patches < <(find "$PATCH_ROOT" -type f \( -name '*.patch' -o -name '*.diff' \) | sort)
if [[ "${#patches[@]}" -eq 0 ]]; then
  echo "No patches found under ${PATCH_ROOT}; upstream source remains unmodified."
  exit 0
fi

cd "$SOURCE_DIR"
for patch_file in "${patches[@]}"; do
  echo "Applying ${patch_file}"
  if [[ -d .git ]]; then
    git am --3way "$patch_file"
  else
    patch -p1 --forward --input "$patch_file"
  fi
done

echo "Applied ${#patches[@]} patches."
