#!/usr/bin/env bash
set -euo pipefail

SOURCE_DIR=""
TARGET="linux-x64"
ARTIFACT=0
SKIP_CONFIG=0
REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

usage() {
  cat <<USAGE
Usage: $0 --source DIR --target linux-x64|win-x64|macos-universal [--artifact] [--skip-config-install]

Stages OpenBook branding into the source tree, runs Firefox's mach build with the
matching mozconfig, then installs the OpenBook AutoConfig + policies into the built
dist. Fails closed if branding, the dist, or the config cannot be installed.

  --skip-config-install   Build only; do NOT install the OpenBook security config.
                          Local development ONLY — such a build MUST NOT be released
                          (it has no telemetry-off / hardening layer).
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
    --skip-config-install)
      SKIP_CONFIG=1
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

# Stage the OpenBook branding into the source tree so the branding patch's
# MOZ_BRANDING_DIRECTORY default and the mozconfig --with-branding both resolve to
# a populated browser/branding/openbook (Phase 1; Build Plan §13).
branding_src="$REPO_ROOT/branding/openbook"
branding_dst="$SOURCE_DIR/browser/branding/openbook"
if [[ ! -d "$branding_src" ]]; then
  echo "OpenBook branding source not found: $branding_src" >&2
  exit 4
fi
echo "Staging OpenBook branding -> $branding_dst"
mkdir -p "$branding_dst"
cp -R "$branding_src/." "$branding_dst/"

cd "$SOURCE_DIR"
export MOZCONFIG="$MOZCONFIG_PATH"
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1735689600}"

echo "Building target ${TARGET} with MOZCONFIG=${MOZCONFIG}."
./mach build

if [[ "$ARTIFACT" -eq 1 ]]; then
  objdir="obj-openbook-artifact"
else
  objdir="obj-openbook-${TARGET}"
fi

if [[ "$SKIP_CONFIG" -eq 1 ]]; then
  echo "WARNING: --skip-config-install set; OpenBook AutoConfig + policies were NOT installed." >&2
  echo "This is a development-only build and MUST NOT be released." >&2
  exit 0
fi

# Install the OpenBook settings layer into the built dist. Fail closed: never
# report success for a build that lacks the hardening config — a silent skip could
# ship a browser with telemetry on and no enterprise policies.
dist_dir=""
for cand in "$SOURCE_DIR/$objdir/dist/bin" "$SOURCE_DIR/$objdir"/dist/*.app/Contents/Resources; do
  if [[ -d "$cand" ]]; then dist_dir="$cand"; break; fi
done
if [[ -z "$dist_dir" ]]; then
  echo "Error: built dist not found under $SOURCE_DIR/$objdir/dist; OpenBook config was NOT installed." >&2
  echo "Refusing to report success without the security config (use --skip-config-install for dev only)." >&2
  exit 4
fi

"$REPO_ROOT/build/scripts/install-config.sh" --dist "$dist_dir"
echo "OpenBook build complete for ${TARGET}: branding staged; AutoConfig + policies installed into $dist_dir."
