#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — per-OS signing orchestration (Build Plan §5/§8/§11).
#
# HARD STANCE (Build Plan §11): signing keys live ONLY in an HSM / hardware token
# / platform signing service. They are NEVER stored in this repository, NEVER in
# plaintext CI variables, and are NEVER generated or embedded by this script.
# This script orchestrates signing by REFERENCING a key handle supplied by the
# environment (an HSM key id, a certificate thumbprint, a Developer ID identity).
# If the required handle or tool is absent, it FAILS CLOSED (nonzero, with a clear
# message of what is needed) rather than producing an unsigned or partially-signed
# artifact.
#
# Per-OS:
#   Linux   -> GPG detached signature (.asc/.sig) + SHA-256 checksum.
#              Key id from $OPENBOOK_GPG_KEY_ID (refuse if unset).
#   Windows -> Authenticode via signtool (Windows) or osslsigncode (cross),
#              using a cert handle from the environment / HSM
#              ($OPENBOOK_WIN_CERT_THUMBPRINT or $OPENBOOK_WIN_CERT) (refuse if unset).
#   macOS   -> codesign (Developer ID) + notarytool submit + stapler staple.
#              Identity from $OPENBOOK_MACOS_IDENTITY (refuse if unset);
#              notarization creds from $OPENBOOK_NOTARY_PROFILE (refuse if unset).
#
# Usage:
#   sign.sh --target TARGET --artifact PATH
#   sign.sh -h
#
#   --target     linux-x64 | win-x64 | macos-universal
#   --artifact   path to the file to sign (produced by package.sh)
#
# Exit codes: 0 success; 2 usage error; 3 missing artifact; 4 missing signing
# tool; 5 missing key handle/credential (fail closed); 6 signing step failed.

set -euo pipefail

PROG="$(basename "$0")"

usage() {
  cat <<USAGE
$PROG — OpenBook per-OS signing orchestration

Usage:
  $PROG --target TARGET --artifact PATH
  $PROG -h | --help

Options:
  --target     linux-x64 | win-x64 | macos-universal
  --artifact   path to the artifact to sign (from build/scripts/package.sh)

Key handles (supplied by the environment; NEVER stored in repo/CI plaintext):
  Linux:   OPENBOOK_GPG_KEY_ID            GPG key id / fingerprint (in an agent/HSM).
  Windows: OPENBOOK_WIN_CERT_THUMBPRINT   Authenticode cert thumbprint in the cert store/HSM,
           or OPENBOOK_WIN_CERT           a PKCS#11/HSM cert reference for osslsigncode.
  macOS:   OPENBOOK_MACOS_IDENTITY        Developer ID Application identity (in the Keychain),
           OPENBOOK_NOTARY_PROFILE        notarytool keychain profile name for notarization.

This script FAILS CLOSED: a missing tool or key handle aborts with a nonzero
exit. It never embeds, generates, or writes signing keys.
USAGE
}

die() {
  local code="$1"
  shift
  echo "$PROG: error: $*" >&2
  exit "$code"
}

require_tool() {
  local tool="$1" hint="$2"
  if ! command -v "$tool" >/dev/null 2>&1; then
    die 4 "required signing tool '$tool' not found. $hint"
  fi
}

require_env() {
  # require_env VARNAME HINT  -- fail closed if the env var is empty/unset.
  local name="$1" hint="$2"
  local val="${!name:-}"
  if [[ -z "$val" ]]; then
    die 5 "required key handle '$name' is not set. $hint (keys live in HSM/hardware tokens only)"
  fi
}

TARGET=""
ARTIFACT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target)
      [[ $# -ge 2 ]] || die 2 "--target requires an argument"
      TARGET="$2"
      shift 2
      ;;
    --artifact)
      [[ $# -ge 2 ]] || die 2 "--artifact requires a path argument"
      ARTIFACT="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      die 2 "unknown argument: $1 (try -h)"
      ;;
  esac
done

[[ -n "$TARGET" ]] || die 2 "--target is required (try -h)"
[[ -n "$ARTIFACT" ]] || die 2 "--artifact is required (try -h)"

case "$TARGET" in
  linux-x64|win-x64|macos-universal) ;;
  *) die 2 "unknown --target '$TARGET' (linux-x64|win-x64|macos-universal)" ;;
esac

[[ -f "$ARTIFACT" ]] || die 3 "artifact not found: '$ARTIFACT'"

echo "$PROG: signing target=$TARGET artifact=$ARTIFACT"

case "$TARGET" in
  linux-x64)
    # GPG detached signature + SHA-256 checksum. The private key must be held by
    # a gpg-agent backed by an HSM/hardware token; we only reference its id.
    require_tool gpg "Install GnuPG; the private key must live in a hardware-backed gpg-agent, not on disk."
    require_env OPENBOOK_GPG_KEY_ID "Export the GPG key id/fingerprint of the hardware-backed release key."
    # Checksum tool: prefer sha256sum, fall back to 'shasum -a 256'. Hash the
    # BASENAME from inside the artifact's directory so the .sha256 contains a
    # relative name a downloader can verify with `sha256sum -c` (a build-host
    # absolute path in the file would never match on their machine).
    artifact_dir="$(dirname -- "$ARTIFACT")"
    artifact_name="$(basename -- "$ARTIFACT")"
    if command -v sha256sum >/dev/null 2>&1; then
      echo "$PROG: writing SHA-256 checksum -> ${ARTIFACT}.sha256"
      (cd "$artifact_dir" && sha256sum "$artifact_name" > "${artifact_name}.sha256") \
        || die 6 "sha256sum failed"
    elif command -v shasum >/dev/null 2>&1; then
      echo "$PROG: writing SHA-256 checksum -> ${ARTIFACT}.sha256"
      (cd "$artifact_dir" && shasum -a 256 "$artifact_name" > "${artifact_name}.sha256") \
        || die 6 "shasum failed"
    else
      die 4 "no SHA-256 tool found (need 'sha256sum' or 'shasum')."
    fi
    echo "$PROG: creating detached GPG signature -> ${ARTIFACT}.asc (key: $OPENBOOK_GPG_KEY_ID)"
    gpg --batch --yes --local-user "$OPENBOOK_GPG_KEY_ID" \
        --armor --detach-sign --output "${ARTIFACT}.asc" "$ARTIFACT" \
      || die 6 "gpg detached-sign failed"
    echo "$PROG: Linux signing complete (.sha256 + .asc)."
    echo "$PROG: NOTE: deb/rpm repo signing (Release.gpg / rpm --addsign) is a separate repo-publish step."
    ;;

  win-x64)
    # Authenticode. On Windows use signtool with a cert thumbprint resolved from
    # the cert store / HSM. Cross-platform fallback: osslsigncode with a PKCS#11
    # HSM reference. We never accept a raw key file path.
    #
    # FAIL CLOSED until the invocation is implemented and validated on a real
    # Windows signing host: exiting 0 here would leave the artifact UNSIGNED
    # while telling the pipeline it was signed — exactly what this script's
    # contract forbids. Intended invocations, for the implementer:
    #   signtool sign /sha1 "$OPENBOOK_WIN_CERT_THUMBPRINT" /fd sha256 \
    #     /tr <RFC3161-timestamp-url> /td sha256 "<artifact>"
    #   osslsigncode sign -pkcs11module <hsm.so> -certs <cert> -h sha256 \
    #     -t <timestamp-url> -in "<artifact>" -out "<artifact>.signed"
    if command -v signtool >/dev/null 2>&1; then
      require_env OPENBOOK_WIN_CERT_THUMBPRINT \
        "Export the Authenticode cert thumbprint present in the Windows cert store / HSM."
    elif command -v osslsigncode >/dev/null 2>&1; then
      require_env OPENBOOK_WIN_CERT \
        "Export a PKCS#11/HSM certificate reference for osslsigncode (never a plaintext .pfx in repo/CI)."
    else
      die 4 "no Authenticode tool found (need 'signtool' on Windows or 'osslsigncode' cross-platform)."
    fi
    die 6 "Windows Authenticode signing not implemented yet — refusing to report an unsigned artifact as signed (fail closed)"
    ;;

  macos-universal)
    # codesign with a Developer ID identity (in the Keychain), then notarize and
    # staple. Gatekeeper blocks un-notarized apps, so all three are required.
    #
    # FAIL CLOSED until implemented on a real macOS signing host (same contract
    # as win-x64 above). Intended sequence, for the implementer:
    #   codesign --force --options runtime --timestamp \
    #     --sign "$OPENBOOK_MACOS_IDENTITY" "<artifact>"
    #   xcrun notarytool submit "<artifact>" \
    #     --keychain-profile "$OPENBOOK_NOTARY_PROFILE" --wait
    #   xcrun stapler staple "<artifact>"
    require_tool codesign "Run on macOS with Xcode command line tools (provides codesign)."
    require_tool xcrun "Run on macOS with Xcode command line tools (provides xcrun/notarytool/stapler)."
    require_env OPENBOOK_MACOS_IDENTITY \
      "Export the 'Developer ID Application' identity name present in the signing Keychain."
    require_env OPENBOOK_NOTARY_PROFILE \
      "Export the notarytool keychain profile name holding the Apple notarization credentials."
    die 6 "macOS codesign/notarize/staple not implemented yet — refusing to report an unsigned artifact as signed (fail closed)"
    ;;
esac

echo "$PROG: signing orchestration done for target=$TARGET."
