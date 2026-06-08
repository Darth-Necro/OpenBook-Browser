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
    --patch-root)
      require_value "$1" "$#" "${2:-}"
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

# Apply phases in the documented order: branding, then privacy, then features,
# sorting within each phase. A bare lexicographic sort over the whole tree would
# order the directories branding/features/privacy, contradicting that contract.
patch_phase_dirs=(branding privacy features)
patches=()
for phase in "${patch_phase_dirs[@]}"; do
  [[ -d "$PATCH_ROOT/$phase" ]] || continue
  while IFS= read -r patch_file; do
    [[ -n "$patch_file" ]] && patches+=("$patch_file")
  done < <(find "$PATCH_ROOT/$phase" -type f \( -name '*.patch' -o -name '*.diff' \) | sort)
done
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
