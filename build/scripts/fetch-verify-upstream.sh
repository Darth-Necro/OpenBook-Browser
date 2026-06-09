#!/usr/bin/env bash
set -euo pipefail

# Mozilla publishes release source tarballs and SHA256 manifests under:
# https://ftp.mozilla.org/pub/firefox/releases/<version>/
# This script verifies the signed SHA256 manifest, pins the signer fingerprint
# to a known Mozilla release-signing key, and verifies the source tarball
# checksum before extracting. It fails closed if any step cannot run.

FIREFOX_VERSION="145.0.2"
BASE_URL="https://ftp.mozilla.org/pub/firefox/releases/${FIREFOX_VERSION}"
SOURCE_NAME="firefox-${FIREFOX_VERSION}.source.tar.xz"
DEST_DIR="${OPENBOOK_UPSTREAM_DIR:-$(pwd)/upstream}"
EXTRACT=1

# Comma-separated list of accepted signer fingerprints (no spaces).
# Primary: Mozilla Software Releases <release@mozilla.com>, key 0x61B7B526D98F0353
# Override via OPENBOOK_EXPECTED_KEY_FPRS for key rotation.
DEFAULT_EXPECTED_KEY_FPRS="14F26682D0916CDD81E37B6D61B7B526D98F0353"
EXPECTED_KEY_FPRS="${OPENBOOK_EXPECTED_KEY_FPRS:-$DEFAULT_EXPECTED_KEY_FPRS}"

usage() {
  cat <<USAGE
Usage: $0 [--dest DIR] [--no-extract]

Environment:
  OPENBOOK_UPSTREAM_GPG_KEYRING  Required path to a GPG keyring containing
                                 Mozilla release signing keys.
  OPENBOOK_UPSTREAM_DIR          Default destination directory when --dest
                                 is omitted.
  OPENBOOK_EXPECTED_KEY_FPRS     Optional comma-separated fingerprint allowlist
                                 (default pins the current Mozilla release key).
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
require_cmd awk
require_cmd sed
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
# Resolve to an absolute path NOW: we `cd` into DEST_DIR below, where a
# relative path would dangle — or worse, a bare filename would be resolved by
# gpgv against ~/.gnupg, silently verifying with a DIFFERENT keyring than the
# one we just checked.
OPENBOOK_UPSTREAM_GPG_KEYRING="$(realpath -- "$OPENBOOK_UPSTREAM_GPG_KEYRING")" || {
  echo "Could not resolve keyring path." >&2
  exit 3
}

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

echo "Verifying Mozilla SHA256 manifest signature and pinning signer fingerprint."
status_file="$(mktemp)"
trap 'rm -f "$status_file"' EXIT
if ! gpgv --status-fd=3 --keyring "$OPENBOOK_UPSTREAM_GPG_KEYRING" SHA256SUMS.asc SHA256SUMS 3>"$status_file"; then
  echo "gpgv signature verification failed." >&2
  exit 5
fi
# VALIDSIG <fpr> <sig-creation-date> <sig-timestamp> <expire-timestamp> ...
actual_fpr="$(awk '/^\[GNUPG:\] VALIDSIG / { print $3; exit }' "$status_file")"
if [[ -z "$actual_fpr" ]]; then
  echo "gpgv produced no VALIDSIG line; cannot pin signer fingerprint." >&2
  exit 5
fi
matched=0
IFS=',' read -r -a expected_arr <<<"$EXPECTED_KEY_FPRS"
for fp in "${expected_arr[@]}"; do
  if [[ "$fp" == "$actual_fpr" ]]; then
    matched=1
    break
  fi
done
if [[ "$matched" -ne 1 ]]; then
  echo "Signer fingerprint ${actual_fpr} is not in the expected allowlist (${EXPECTED_KEY_FPRS})." >&2
  exit 5
fi
echo "Signer fingerprint pinned: ${actual_fpr}"

echo "Verifying source tarball checksum."
expected_line="$(awk -v path="source/${SOURCE_NAME}" '$2 == path { print $0 }' SHA256SUMS)"
if [[ -z "$expected_line" ]]; then
  echo "No SHA256SUMS entry found for source/${SOURCE_NAME}" >&2
  exit 4
fi
printf '%s\n' "$expected_line" | sed "s#source/${SOURCE_NAME}#${SOURCE_NAME}#" | sha256sum --check --strict

if [[ "$EXTRACT" -eq 1 ]]; then
  echo "Extracting ${SOURCE_NAME}."
  tar --no-same-owner --no-same-permissions -xf "$SOURCE_NAME"
fi

echo "Verified Firefox ${FIREFOX_VERSION} source in ${DEST_DIR}."
