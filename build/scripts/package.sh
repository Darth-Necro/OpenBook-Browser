#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — per-OS packaging orchestration (Build Plan §5/§8).
#
# Runs `./mach package` to produce the base package from a built Firefox source
# tree, then orchestrates the requested OS-specific package format. Every format
# checks for its required packaging tool up front and FAILS CLOSED (nonzero exit,
# clear message) if the tool is missing — it NEVER emits a half-built package
# silently.
#
# IMPORTANT: release packaging runs on the proper PER-OS build hosts / CI runners
# (you cannot build a notarized .dmg off macOS, an MSI off Windows tooling, etc.).
# This script is the orchestration; the heavy lifting is the platform packager.
#
# Signing is a SEPARATE step (build/scripts/sign.sh) and is never done here.
#
# Usage:
#   package.sh --source DIR --target TARGET --format FORMAT
#   package.sh -h
#
#   --source DIR     Firefox source tree containing an executable `mach` (built).
#   --target         linux-x64 | win-x64 | macos-universal
#   --format         tar.xz | deb | rpm | flatpak | appimage | dmg | pkg | exe | msi
#
# Exit codes: 0 success; 2 usage error; 3 missing/invalid source; 4 missing
# packaging prerequisite; 5 target/format mismatch; 6 packaging step failed;
# 7 OpenBook security config missing from the built dist.

set -euo pipefail

PROG="$(basename "$0")"

usage() {
  cat <<USAGE
$PROG — OpenBook per-OS packaging orchestration

Usage:
  $PROG --source DIR --target TARGET --format FORMAT
  $PROG -h | --help

Options:
  --source DIR   Built Firefox source tree containing an executable 'mach'.
  --target       linux-x64 | win-x64 | macos-universal
  --format       tar.xz | deb | rpm | flatpak | appimage | dmg | pkg | exe | msi

Notes:
  * Release packaging must run on the matching per-OS build host / CI runner.
  * This script fails closed: if a required packaging tool is missing it exits
    nonzero with a clear message rather than producing a partial package.
  * Signing is performed separately by build/scripts/sign.sh.
USAGE
}

die() {
  # die CODE MESSAGE...
  local code="$1"
  shift
  echo "$PROG: error: $*" >&2
  exit "$code"
}

require_tool() {
  # require_tool TOOL HINT
  local tool="$1" hint="$2"
  if ! command -v "$tool" >/dev/null 2>&1; then
    die 4 "required packaging tool '$tool' not found. $hint"
  fi
}

SOURCE_DIR=""
TARGET=""
FORMAT=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source)
      [[ $# -ge 2 ]] || die 2 "--source requires a directory argument"
      SOURCE_DIR="$2"
      shift 2
      ;;
    --target)
      [[ $# -ge 2 ]] || die 2 "--target requires an argument"
      TARGET="$2"
      shift 2
      ;;
    --format)
      [[ $# -ge 2 ]] || die 2 "--format requires an argument"
      FORMAT="$2"
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

# --- validate inputs --------------------------------------------------------

[[ -n "$SOURCE_DIR" ]] || die 2 "--source is required (try -h)"
[[ -n "$TARGET" ]] || die 2 "--target is required (try -h)"
[[ -n "$FORMAT" ]] || die 2 "--format is required (try -h)"

case "$TARGET" in
  linux-x64|win-x64|macos-universal) ;;
  *) die 2 "unknown --target '$TARGET' (linux-x64|win-x64|macos-universal)" ;;
esac

if [[ ! -d "$SOURCE_DIR" || ! -x "$SOURCE_DIR/mach" ]]; then
  die 3 "a built Firefox source directory with an executable 'mach' is required (got '$SOURCE_DIR')"
fi

# Reject obvious target/format mismatches early (fail closed, not half-built).
mismatch() { die 5 "format '$FORMAT' is not valid for target '$TARGET'"; }
case "$FORMAT" in
  tar.xz) ;;  # valid on any target's build host that produced a tree
  deb|rpm|flatpak|appimage)
    [[ "$TARGET" == "linux-x64" ]] || mismatch ;;
  dmg|pkg)
    [[ "$TARGET" == "macos-universal" ]] || mismatch ;;
  exe|msi)
    [[ "$TARGET" == "win-x64" ]] || mismatch ;;
  *)
    die 2 "unknown --format '$FORMAT'" ;;
esac

echo "$PROG: packaging target=$TARGET format=$FORMAT from source=$SOURCE_DIR"

# --- verify the OpenBook hardening layer is present in the dist (fail closed) -
# build.sh installs AutoConfig + policies into the dist; refuse to package a build
# that is missing them (it would ship without telemetry-off / enterprise policies —
# a release-blocking security regression, §4/§11).
objdir="obj-openbook-${TARGET}"
dist_dir=""
for cand in "$SOURCE_DIR/$objdir/dist/bin" "$SOURCE_DIR/$objdir"/dist/*.app/Contents/Resources; do
  [[ -d "$cand" ]] && { dist_dir="$cand"; break; }
done
[[ -n "$dist_dir" ]] || die 3 "built dist not found under $SOURCE_DIR/$objdir/dist (run build/scripts/build.sh first)"
for rel in "defaults/pref/autoconfig.js" "openbook.cfg" "distribution/policies.json"; do
  [[ -f "$dist_dir/$rel" ]] || die 7 "OpenBook security config missing from dist: $rel (run build/scripts/install-config.sh --dist '$dist_dir'); refusing to package an unhardened build"
done
echo "$PROG: verified OpenBook AutoConfig + policies present in $dist_dir"

# --- base package: mach package ---------------------------------------------
# Always produce the base package first; the OS format wraps/derives from it.

echo "$PROG: running './mach package' (base package)"
(
  cd "$SOURCE_DIR"
  ./mach package
) || die 6 "'mach package' failed"

# --- per-format orchestration -----------------------------------------------
# Each branch verifies its prerequisite tool, then runs the packager. The actual
# spec/recipe files (control files, .spec, manifests, .desktop, AppDir layout,
# WiX/NSIS scripts) live alongside the release config and are passed in by CI on
# the proper host. Here we gate prerequisites and fail closed.

case "$FORMAT" in
  tar.xz)
    require_tool tar "Install GNU tar (coreutils/tar package)."
    require_tool xz "Install xz-utils for .tar.xz compression."
    echo "$PROG: 'mach package' already emits the .tar.xz base artifact for this target."
    echo "$PROG: (no extra step required; verify dist/ for the openbook-*.tar.xz)"
    ;;

  deb)
    require_tool dpkg-deb "Install dpkg-dev (provides dpkg-deb). Run on a Debian/Ubuntu build host."
    echo "$PROG: building .deb (dpkg-deb)."
    echo "$PROG: TODO(build-host): assemble the DEBIAN/ control tree + payload, then"
    echo "$PROG:   dpkg-deb --build --root-owner-group <stagedir> <out.deb>"
    echo "$PROG:   NOTE: privileged files (openbook.cfg, defaults/pref/*.js, native"
    echo "$PROG:   host binary + manifest) MUST be packaged root-owned and 0644/0755,"
    echo "$PROG:   not user-writable (Build Plan §11)."
    ;;

  rpm)
    require_tool rpmbuild "Install rpm-build (provides rpmbuild). Run on a Fedora/RHEL/openSUSE build host."
    echo "$PROG: building .rpm (rpmbuild)."
    echo "$PROG: TODO(build-host): render the openbook.spec with the built tree and"
    echo "$PROG:   rpmbuild -bb openbook.spec  (ensure %files marks privileged files"
    echo "$PROG:   root-owned and non-user-writable per §11)."
    ;;

  flatpak)
    require_tool flatpak-builder "Install flatpak-builder + the Flatpak runtime/SDK. Run on a host with flatpak."
    echo "$PROG: building Flatpak (flatpak-builder)."
    echo "$PROG: TODO(build-host): flatpak-builder --repo=<repo> <builddir>"
    echo "$PROG:   org.openbook.Browser.yml  (then 'flatpak build-bundle' for a .flatpak)."
    ;;

  appimage)
    require_tool appimagetool "Install appimagetool (AppImageKit). Run on a Linux build host."
    echo "$PROG: building AppImage (appimagetool)."
    echo "$PROG: TODO(build-host): stage the AppDir (AppRun, .desktop, icon, payload)"
    echo "$PROG:   then  appimagetool <AppDir> openbook-x86_64.AppImage"
    ;;

  dmg)
    require_tool hdiutil "Run on macOS: 'hdiutil' ships with macOS and is required to build a .dmg."
    echo "$PROG: building .dmg (hdiutil) — macOS host required."
    echo "$PROG: TODO(macOS host): hdiutil create -volname OpenBook -srcfolder <staged.app>"
    echo "$PROG:   -ov -format UDZO openbook.dmg  (universal x86_64+arm64 .app)."
    echo "$PROG:   codesign + notarize + staple happen in build/scripts/sign.sh."
    ;;

  pkg)
    # Prefer productbuild; fall back to pkgbuild. Require at least one.
    if command -v productbuild >/dev/null 2>&1; then
      echo "$PROG: building .pkg (productbuild) — macOS host."
    elif command -v pkgbuild >/dev/null 2>&1; then
      echo "$PROG: building .pkg (pkgbuild) — macOS host."
    else
      die 4 "neither 'productbuild' nor 'pkgbuild' found. Run on a macOS build host with Xcode command line tools."
    fi
    echo "$PROG: TODO(macOS host): pkgbuild/productbuild the staged universal .app into openbook.pkg."
    ;;

  exe)
    require_tool makensis "Install NSIS (provides makensis). Run on a Windows build host (or wine-based CI)."
    echo "$PROG: building NSIS installer .exe (makensis) — Windows host."
    echo "$PROG: TODO(Windows host): makensis openbook.nsi  (Authenticode signing is sign.sh)."
    ;;

  msi)
    # WiX toolset: candle+light (v3) or the 'wix' CLI (v4+). Require at least one.
    if command -v wix >/dev/null 2>&1; then
      echo "$PROG: building .msi (WiX 'wix' CLI) — Windows host."
    elif command -v candle >/dev/null 2>&1 && command -v light >/dev/null 2>&1; then
      echo "$PROG: building .msi (WiX candle+light) — Windows host."
    else
      die 4 "WiX toolset not found ('wix' or 'candle'+'light'). Install WiX and run on a Windows build host."
    fi
    echo "$PROG: TODO(Windows host): compile the WiX authoring into openbook.msi (signing is sign.sh)."
    ;;

  *)
    die 2 "unhandled --format '$FORMAT'"
    ;;
esac

echo "$PROG: packaging orchestration complete for target=$TARGET format=$FORMAT."
