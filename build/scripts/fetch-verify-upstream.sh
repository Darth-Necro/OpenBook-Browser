#!/usr/bin/env bash
set -euo pipefail

# Mozilla publishes release source tarballs and SHA256 manifests under:
# https://ftp.mozilla.org/pub/firefox/releases/<version>/
# This script verifies both the signed SHA256 manifest and the source tarball
# before extracting. It intentionally fails if signature verification cannot run.

FIREFOX_VERSION="145.0.2"
BASE_URL="https://ftp.mozilla.org/pub/firefox/releases/${FIREFOX_VERSION}"
SOURCE_NAME="firefox-${FIREFOX_VERSION}.source.tar.xz"
DEST_DIR="${OPENBOOK_UPSTREAM_DIR:-$(pwd)/upstream}"
EXTRACT=1

usage() {
  cat <<USAGE
Usage: $0 [--dest DIR] [--no-extract]

Environment:
  OPENBOOK_UPSTREAM_GPG_KEYRING  Required path to a GPG keyring containing Mozilla release signing keys.
  OPENBOOK_UPSTREAM_DIR          Default destination directory when --dest is omitted.
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
    --dest)
      require_value "$1" "$#" "${2:-}"
      DEST_DIR="$2"
      shift 2
      ;;
    --no-extract)
      EXTRACT=0
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

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Required command not found: $1" >&2
    exit 127
  fi
}

require_cmd curl
require_cmd sha256sum
require_cmd gpgv
if [[ "$EXTRACT" -eq 1 ]]; then
  require_cmd tar
fi

if [[ -z "${OPENBOOK_UPSTREAM_GPG_KEYRING:-}" ]]; then
  echo "OPENBOOK_UPSTREAM_GPG_KEYRING is required for Mozilla manifest signature verification." >&2
  exit 3
fi
if [[ ! -r "$OPENBOOK_UPSTREAM_GPG_KEYRING" ]]; then
  echo "GPG keyring is not readable: $OPENBOOK_UPSTREAM_GPG_KEYRING" >&2
  exit 3
fi

mkdir -p "$DEST_DIR"
cd "$DEST_DIR"

download() {
  local url="$1"
  local out="$2"
  curl --fail --location --proto '=https' --tlsv1.2 --retry 3 --retry-delay 2 --output "${out}.tmp" "$url"
  mv "${out}.tmp" "$out"
}

echo "Fetching Firefox ${FIREFOX_VERSION} source and signed checksum manifest."
download "${BASE_URL}/source/${SOURCE_NAME}" "$SOURCE_NAME"
download "${BASE_URL}/SHA256SUMS" SHA256SUMS
download "${BASE_URL}/SHA256SUMS.asc" SHA256SUMS.asc

echo "Verifying Mozilla SHA256 manifest signature."
gpgv --keyring "$OPENBOOK_UPSTREAM_GPG_KEYRING" SHA256SUMS.asc SHA256SUMS

echo "Verifying source tarball checksum."
expected_line="$(awk -v path="source/${SOURCE_NAME}" '$2 == path { print $0 }' SHA256SUMS)"
if [[ -z "$expected_line" ]]; then
  echo "No SHA256SUMS entry found for source/${SOURCE_NAME}" >&2
  exit 4
fi
printf '%s\n' "$expected_line" | sed "s#source/${SOURCE_NAME}#${SOURCE_NAME}#" | sha256sum --check --strict

if [[ "$EXTRACT" -eq 1 ]]; then
  echo "Extracting ${SOURCE_NAME}."
  tar -xf "$SOURCE_NAME"
fi

echo "Verified Firefox ${FIREFOX_VERSION} source in ${DEST_DIR}."
