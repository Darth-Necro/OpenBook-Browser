#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR=""
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PATCH_ROOT="${REPO_ROOT}/patches"
BRANDING_TREE="${REPO_ROOT}/branding/openbook"

usage() {
  cat <<USAGE
Usage: $0 --source DIR [--patch-root DIR]

Applies the ordered patch series declared in patches/SERIES when present
(one path per line, relative to patches/; '#' comments and blank lines
ignored), falling back to the documented phase order — branding, then
privacy, then features — sorting within each phase under LC_ALL=C.

After patches apply, if branding/openbook/ exists, its contents are mirrored
into <source>/browser/branding/openbook/ to supply the binary brand assets
and default-prefs.js that the branding patch references but does not embed.
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

# Build the patch list. An explicit patches/SERIES is authoritative (it must
# list EVERY patch, in apply order); without it, fall back to the documented
# phase order. A bare lexicographic sort over the whole tree would order the
# directories branding/features/privacy, contradicting that contract.
patches=()
if [[ -f "${PATCH_ROOT}/SERIES" ]]; then
  while IFS= read -r line; do
    # Strip comments and surrounding whitespace; skip blanks.
    line="${line%%#*}"
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    [[ -z "$line" ]] && continue
    p="${PATCH_ROOT}/${line}"
    if [[ ! -f "$p" ]]; then
      echo "SERIES entry not found: ${line}" >&2
      exit 4
    fi
    patches+=("$p")
  done < "${PATCH_ROOT}/SERIES"
else
  patch_phase_dirs=(branding privacy features)
  for phase in "${patch_phase_dirs[@]}"; do
    [[ -d "$PATCH_ROOT/$phase" ]] || continue
    while IFS= read -r patch_file; do
      [[ -n "$patch_file" ]] && patches+=("$patch_file")
    done < <(find "$PATCH_ROOT/$phase" -type f \( -name '*.patch' -o -name '*.diff' \) | LC_ALL=C sort)
  done
fi

if [[ "${#patches[@]}" -eq 0 ]]; then
  echo "No patches found under ${PATCH_ROOT}; upstream source remains unmodified."
else
  cd "$SOURCE_DIR"
  for patch_file in "${patches[@]}"; do
    echo "Applying ${patch_file}"
    if [[ -d .git ]]; then
      git am --3way "$patch_file"
    else
      # --fuzz=0: a privacy/security hunk applying at a drifted offset is a
      # silent semantic change; demand exact context or fail hard.
      patch -p1 --forward --fuzz=0 --input "$patch_file"
    fi
  done
  echo "Applied ${#patches[@]} patches."
fi

# Mirror in-repo branding assets after patches apply. The branding patch
# creates browser/branding/openbook/jar.mn etc. but the binary icons and
# default-prefs.js live in this repo so they can be swapped without rewriting
# the patch.
if [[ -d "$BRANDING_TREE" ]]; then
  echo "Syncing branding/openbook -> ${SOURCE_DIR}/browser/branding/openbook"
  mkdir -p "${SOURCE_DIR}/browser/branding/openbook"
  cp -a "${BRANDING_TREE}/." "${SOURCE_DIR}/browser/branding/openbook/"
fi
