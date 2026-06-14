#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — component artifact assembly (Build Plan §8/§9, ADR-0017).
#
# Produces the release artifacts this repository can build WITHOUT a Firefox
# build host, deterministically:
#
#   * openbook-<ext>-<version>.xpi            one per bundled extension, built
#                                             from manifest + pages + compiled
#                                             dist/ (requires `npm run build`
#                                             to have run — FAILS CLOSED if the
#                                             compiled output is missing).
#   * openbook-<host>-<version>-linux-x64     release native-host binaries
#     + org.openbook.<host>.json              (cargo build --release --locked)
#                                             with their native-messaging
#                                             manifests.
#   * openbook-settings-<version>.tar.xz      the settings overlay (patches/,
#                                             config/, branding/, mozconfigs,
#                                             build scripts) that turns a
#                                             verified upstream tree into
#                                             OpenBook — the LibreWolf-style
#                                             "settings" artifact.
#   * SHA256SUMS                              over everything above.
#
# Determinism: file timestamps are normalized to SOURCE_DATE_EPOCH (defaulting
# to the HEAD commit time), archive entries are sorted, and ownership is
# normalized — re-running on the same commit yields byte-identical archives.
#
# Signing is NOT done here (build/scripts/sign.sh, maintainer hardware only).
#
# Usage:
#   package-components.sh [--out DIR] [--skip-native] [--with-sbom]
#
#   --out DIR       Output directory (default: <repo>/dist/release). Created if
#                   absent; existing artifact files for this version are
#                   overwritten.
#   --skip-native   Skip the Rust native-host build (for hosts without cargo;
#                   a RELEASE run must not use this — the release workflow
#                   builds everything).
#   --with-sbom     Run ci/sbom.sh into the output directory before
#                   checksumming. STRICT: any SBOM warning (missing tool,
#                   skipped component) fails the run — a release SBOM must be
#                   complete (Build Plan §11).
#
# Exit codes: 0 success; 2 usage error; 3 missing prerequisite tool;
# 4 missing/unbuilt component (fail closed); 5 version metadata inconsistent;
# 6 incomplete SBOM under --with-sbom.

set -euo pipefail

PROG="$(basename "$0")"
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." >/dev/null 2>&1 && pwd)"

OUT_DIR="$REPO_ROOT/dist/release"
SKIP_NATIVE=0
WITH_SBOM=0

usage() {
  cat <<USAGE
$PROG — assemble OpenBook component release artifacts (deterministic).

Usage:
  $PROG [--out DIR] [--skip-native]
  $PROG -h | --help

Builds extension XPIs, linux-x64 native-host binaries + manifests, the
settings overlay tarball, and SHA256SUMS. Fails closed on any missing
prerequisite or unbuilt component. Signing is separate (sign.sh).
USAGE
}

die() {
  local code="$1"
  shift
  echo "$PROG: error: $*" >&2
  exit "$code"
}

info() { echo "$PROG: $*"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      [[ $# -ge 2 ]] || die 2 "--out requires a directory argument"
      OUT_DIR="$2"
      shift 2
      ;;
    --skip-native)
      SKIP_NATIVE=1
      shift
      ;;
    --with-sbom)
      WITH_SBOM=1
      shift
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

require_tool() {
  local tool="$1" hint="$2"
  command -v "$tool" >/dev/null 2>&1 || die 3 "required tool '$tool' not found. $hint"
}

require_tool python3 "Install Python 3.9+ (used for deterministic zip assembly)."
require_tool tar "Install GNU tar."
require_tool xz "Install xz-utils."
require_tool sha256sum "Install coreutils."

# --- version metadata (fail closed on drift) ---------------------------------

VERSION_FILE="$REPO_ROOT/VERSION"
[[ -f "$VERSION_FILE" ]] || die 5 "VERSION file not found at repo root"
VERSION="$(tr -d '[:space:]' < "$VERSION_FILE")"
[[ "$VERSION" =~ ^[0-9]+(\.[0-9]+)*-[0-9]+$ ]] \
  || die 5 "VERSION '$VERSION' does not match <firefox-version>-<openbook-build> (e.g. 145.0.2-1)"

UPSTREAM_PIN="$(sed -n 's/^FIREFOX_VERSION="\([^"]*\)".*/\1/p' "$REPO_ROOT/build/scripts/fetch-verify-upstream.sh" | head -1)"
[[ -n "$UPSTREAM_PIN" ]] || die 5 "could not read FIREFOX_VERSION pin from fetch-verify-upstream.sh"
[[ "${VERSION%-*}" == "$UPSTREAM_PIN" ]] \
  || die 5 "VERSION prefix '${VERSION%-*}' does not match the upstream pin '$UPSTREAM_PIN' — re-pin or fix VERSION (see docs/RELEASE-CHECKLIST.md §1)"

# Deterministic timestamp: explicit SOURCE_DATE_EPOCH wins; else HEAD commit time.
if [[ -z "${SOURCE_DATE_EPOCH:-}" ]]; then
  if command -v git >/dev/null 2>&1 && git -C "$REPO_ROOT" rev-parse --git-dir >/dev/null 2>&1; then
    SOURCE_DATE_EPOCH="$(git -C "$REPO_ROOT" log -1 --format=%ct)"
  else
    die 5 "SOURCE_DATE_EPOCH unset and no git history available — set it explicitly for a deterministic build"
  fi
fi
export SOURCE_DATE_EPOCH

mkdir -p "$OUT_DIR"
info "version=$VERSION upstream-pin=$UPSTREAM_PIN SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH"
info "writing artifacts to $OUT_DIR"

# --- extension XPIs (deterministic zip) --------------------------------------
# An XPI ships the manifest, the user-facing pages/styles, and the compiled
# dist/ JS the pages reference — never src/, tests, node_modules, or toolchain
# files. Built with python3 zipfile: sorted entries, fixed timestamps, fixed
# permissions, no "extra" fields — byte-identical across runs.

build_xpi() {
  # build_xpi NAME
  local name="$1"
  local ext_dir="$REPO_ROOT/extensions/$name"
  local out_xpi="$OUT_DIR/openbook-$name-$VERSION.xpi"

  [[ -f "$ext_dir/manifest.json" ]] || die 4 "extension '$name' has no manifest.json"
  [[ -d "$ext_dir/dist" ]] \
    || die 4 "extension '$name' is not built (no dist/): run 'npm --prefix extensions/$name ci && npm --prefix extensions/$name run build' first"

  python3 - "$ext_dir" "$out_xpi" "$SOURCE_DATE_EPOCH" <<'PY'
import os, sys, time, zipfile

ext_dir, out_xpi, epoch = sys.argv[1], sys.argv[2], int(sys.argv[3])
# zip format cannot store pre-1980 timestamps
zip_time = time.gmtime(max(epoch, 315532800))[:6]

entries = []
for root, dirs, files in os.walk(ext_dir):
    rel_root = os.path.relpath(root, ext_dir)
    # prune non-shipping trees
    dirs[:] = sorted(d for d in dirs if d not in ("node_modules", "src", ".git"))
    for f in files:
        rel = os.path.normpath(os.path.join(rel_root, f)).replace(os.sep, "/")
        if rel.startswith("./"):
            rel = rel[2:]
        if rel == ".":
            continue
        top = rel.split("/", 1)[0]
        if top in ("node_modules", "src"):
            continue
        # ship: manifest, pages, styles, compiled dist/ (without sourcemaps)
        ship = (
            rel == "manifest.json"
            or rel.endswith(".html")
            or rel.endswith(".css")
            or (rel.startswith("dist/") and rel.endswith(".js"))
        )
        if ship:
            entries.append(rel)

if "manifest.json" not in entries:
    sys.exit("manifest.json missing from ship set")
if not any(e.startswith("dist/") for e in entries):
    sys.exit("no compiled dist/*.js in ship set")

with zipfile.ZipFile(out_xpi, "w", zipfile.ZIP_DEFLATED) as zf:
    for rel in sorted(entries):
        with open(os.path.join(ext_dir, rel), "rb") as fh:
            data = fh.read()
        zi = zipfile.ZipInfo(rel, date_time=zip_time)
        zi.external_attr = 0o644 << 16
        zi.compress_type = zipfile.ZIP_DEFLATED
        # Pin create_system: ZipInfo defaults it to 0 on Windows and 3 elsewhere,
        # which would store a host-dependent byte and break byte-for-byte repro
        # across OSes (an independent rebuild on macOS/Windows would mismatch a
        # Linux release for otherwise-identical input). Fix it to 3 (Unix).
        zi.create_system = 3
        zf.writestr(zi, data)
print(f"  {os.path.basename(out_xpi)}: {len(entries)} files")
PY
  info "built $(basename "$out_xpi")"
}

for ext in vault-ui proxy-manager ai-sidebar; do
  build_xpi "$ext"
done

# --- native hosts (linux-x64 release binaries + manifests) -------------------

if [[ "$SKIP_NATIVE" -eq 1 ]]; then
  info "skipping native-host build (--skip-native); a release run must build them"
else
  require_tool cargo "Install the Rust toolchain (rustup) to build the native hosts."

  build_native() {
    # build_native CRATE_DIR BIN_NAME MANIFEST_NAME
    local crate="$1" bin="$2" manifest="$3"
    local crate_dir="$REPO_ROOT/native/$crate"
    [[ -f "$crate_dir/Cargo.toml" ]] || die 4 "native host '$crate' not found"
    info "cargo build --release --locked ($crate)"
    cargo build --release --locked --manifest-path "$crate_dir/Cargo.toml" \
      || die 4 "release build failed for '$crate'"
    local built="$crate_dir/target/release/$bin"
    [[ -f "$built" ]] || die 4 "expected release binary missing: $built"
    install -m 0755 "$built" "$OUT_DIR/openbook-$crate-$VERSION-linux-x64"
    install -m 0644 "$crate_dir/manifests/$manifest" "$OUT_DIR/$manifest"
    # Normalize mtimes so SHA256SUMS inputs are stable file content; the
    # binary itself is only bit-reproducible inside the pinned container
    # (tests/repro/), which is where the repro gate runs.
    touch -d "@$SOURCE_DATE_EPOCH" \
      "$OUT_DIR/openbook-$crate-$VERSION-linux-x64" "$OUT_DIR/$manifest"
    info "built openbook-$crate-$VERSION-linux-x64 (+ $manifest)"
  }

  build_native "vault-host" "openbook-vault-host" "org.openbook.vault_host.json"
  build_native "vpn-helper" "openbook-vpn-helper" "org.openbook.vpn_helper.json"
fi

# --- settings overlay tarball -------------------------------------------------
# Everything needed to turn a verified upstream Firefox tree into OpenBook:
# the patch series, the AutoConfig/policy/distribution layer, branding, the
# mozconfigs, and the build scripts. Deterministic tar (sorted, owner 0:0,
# fixed mtime) piped through single-threaded xz.

OVERLAY="$OUT_DIR/openbook-settings-$VERSION.tar.xz"
info "assembling settings overlay -> $(basename "$OVERLAY")"
tar --sort=name \
    --owner=0 --group=0 --numeric-owner \
    --mtime="@$SOURCE_DATE_EPOCH" \
    --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime \
    -C "$REPO_ROOT" \
    -cf - \
    VERSION \
    patches \
    config \
    branding \
    build/mozconfig \
    build/scripts \
  | xz -9 -T1 > "$OVERLAY"
touch -d "@$SOURCE_DATE_EPOCH" "$OVERLAY"

# --- SBOM (strict under --with-sbom) ------------------------------------------
# ci/sbom.sh degrades gracefully for dev environments; a release must not. Any
# WARN line means a component or tool was skipped — fail closed here so an
# incomplete SBOM can never ride into a release unnoticed.

if [[ "$WITH_SBOM" -eq 1 ]]; then
  info "generating release SBOM (strict)"
  sbom_log="$(mktemp)"
  "$REPO_ROOT/ci/sbom.sh" --out "$OUT_DIR" 2> "$sbom_log" || {
    cat "$sbom_log" >&2
    rm -f "$sbom_log"
    die 6 "ci/sbom.sh failed"
  }
  if grep -q 'WARN:' "$sbom_log"; then
    cat "$sbom_log" >&2
    rm -f "$sbom_log"
    die 6 "SBOM generation emitted warnings — a release SBOM must be complete (install the missing tools / build the missing components)"
  fi
  rm -f "$sbom_log"
  # Warnings aside, require the real CycloneDX outputs: sbom.sh's cargo-metadata
  # fallback is fine for dev but is not a release SBOM.
  for sbom in rust-vault-host rust-vpn-helper npm-vault-ui npm-proxy-manager npm-ai-sidebar; do
    [[ -f "$OUT_DIR/$sbom.cdx.json" ]] \
      || die 6 "missing CycloneDX SBOM '$sbom.cdx.json' (install cargo-cyclonedx / npm>=9 and re-run)"
  done
  touch -d "@$SOURCE_DATE_EPOCH" "$OUT_DIR"/*.cdx.json
fi

# --- checksums ----------------------------------------------------------------

info "writing SHA256SUMS"
(
  cd "$OUT_DIR"
  find . -maxdepth 1 -type f ! -name 'SHA256SUMS*' -printf '%P\n' | LC_ALL=C sort \
    | xargs -d '\n' sha256sum > SHA256SUMS
)

info "component assembly complete:"
sed 's/^/  /' "$OUT_DIR/SHA256SUMS"
info "next: sign on maintainer hardware (build/scripts/sign.sh), see docs/RELEASE-CHECKLIST.md §5."
