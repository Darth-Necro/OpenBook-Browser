#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — Software Bill of Materials (SBOM) generation (Build Plan §11).
#
# A release SBOM is REQUIRED (§11 supply chain): every release must publish a
# machine-readable inventory of its dependencies so downstream users and auditors
# can assess supply-chain risk and CVE exposure. This script aggregates a
# CycloneDX SBOM across the project's two dependency ecosystems:
#
#   * Rust   -> `cargo metadata` (and, if installed, `cargo cyclonedx`) for
#               native/vault-host and native/vpn-helper.
#   * npm    -> `npm sbom --sbom-format cyclonedx` per extension (falls back to
#               `npm ls --json` if the npm version predates `npm sbom`).
#
# Output goes to sbom/ (created if absent). The script DEGRADES GRACEFULLY: a
# missing tool or a missing component directory is WARNED and skipped, never
# fatal — so it runs in partial dev environments. (A *release* build should run
# it where all tools are present and treat any warning as a gap to close.)
#
# Usage:
#   ci/sbom.sh [--out DIR]
#   ci/sbom.sh -h
#
# Exit code: 0 always on a normal run (warnings are non-fatal by design); 2 on a
# usage error.

set -euo pipefail

PROG="$(basename "$0")"

# Resolve repo root from this script's location (ci/ is at the repo root).
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/.." >/dev/null 2>&1 && pwd)"

OUT_DIR="$REPO_ROOT/sbom"

usage() {
  cat <<USAGE
$PROG — generate a CycloneDX SBOM for OpenBook (Rust + npm).

Usage:
  $PROG [--out DIR]
  $PROG -h | --help

Options:
  --out DIR   Output directory for SBOM files (default: <repo>/sbom).

Degrades gracefully: missing tools/components are warned and skipped, not fatal.
A release SBOM is required (Build Plan §11); run this where all tools are present.
USAGE
}

warn() { echo "$PROG: WARN: $*" >&2; }
info() { echo "$PROG: $*"; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --out)
      [[ $# -ge 2 ]] || { echo "$PROG: --out requires a directory" >&2; exit 2; }
      OUT_DIR="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "$PROG: unknown argument: $1 (try -h)" >&2
      exit 2
      ;;
  esac
done

mkdir -p "$OUT_DIR"
info "writing SBOM artifacts to $OUT_DIR"

# --- Rust components --------------------------------------------------------

rust_sbom() {
  # rust_sbom NAME RELPATH
  local name="$1" rel="$2"
  local dir="$REPO_ROOT/$rel"
  if [[ ! -f "$dir/Cargo.toml" ]]; then
    warn "Rust component '$name' not found at $rel (no Cargo.toml) — skipping"
    return 0
  fi
  if ! command -v cargo >/dev/null 2>&1; then
    warn "cargo not found — skipping Rust SBOM for '$name'"
    return 0
  fi
  # Preferred: a real CycloneDX SBOM via the cargo-cyclonedx subcommand.
  if cargo cyclonedx --help >/dev/null 2>&1; then
    info "cargo cyclonedx for '$name'"
    if cargo cyclonedx --manifest-path "$dir/Cargo.toml" --format json >/dev/null 2>&1; then
      # cargo-cyclonedx writes <crate>.cdx.json next to the manifest; move it.
      find "$dir" -maxdepth 1 -name '*.cdx.json' -exec mv -f {} "$OUT_DIR/rust-$name.cdx.json" \; 2>/dev/null || true
      info "wrote $OUT_DIR/rust-$name.cdx.json"
      return 0
    fi
    warn "cargo cyclonedx failed for '$name'; falling back to cargo metadata"
  fi
  # Fallback: capture the full resolved dependency graph as cargo metadata JSON.
  # Write to a temp file and publish only on success so a failure leaves no
  # empty/partial file behind.
  info "cargo metadata for '$name' (CycloneDX subcommand not available)"
  local mtmp="$OUT_DIR/.rust-$name.metadata.json.tmp"
  if cargo metadata --manifest-path "$dir/Cargo.toml" --format-version 1 \
       > "$mtmp" 2>/dev/null && [[ -s "$mtmp" ]]; then
    mv -f "$mtmp" "$OUT_DIR/rust-$name.metadata.json"
    info "wrote $OUT_DIR/rust-$name.metadata.json (convert to CycloneDX in CI with cargo-cyclonedx)"
  else
    rm -f "$mtmp"
    warn "cargo metadata failed for '$name' — no Rust SBOM produced for it"
  fi
}

rust_sbom "vault-host" "native/vault-host"
rust_sbom "vpn-helper" "native/vpn-helper"

# --- npm components (extensions) -------------------------------------------

npm_sbom() {
  # npm_sbom NAME RELPATH
  local name="$1" rel="$2"
  local dir="$REPO_ROOT/$rel"
  if [[ ! -f "$dir/package.json" ]]; then
    warn "npm component '$name' not found at $rel (no package.json) — skipping"
    return 0
  fi
  if ! command -v npm >/dev/null 2>&1; then
    warn "npm not found — skipping npm SBOM for '$name'"
    return 0
  fi
  # Preferred: `npm sbom` (npm >= 9). Produces CycloneDX directly. Write to a
  # temp file and only publish it on success, so a failed run never leaves an
  # empty/partial .cdx.json behind.
  info "npm sbom for '$name'"
  local tmp="$OUT_DIR/.npm-$name.cdx.json.tmp"
  if ( cd "$dir" && npm sbom --sbom-format cyclonedx ) > "$tmp" 2>/dev/null && [[ -s "$tmp" ]]; then
    mv -f "$tmp" "$OUT_DIR/npm-$name.cdx.json"
    info "wrote $OUT_DIR/npm-$name.cdx.json"
    return 0
  fi
  rm -f "$tmp"
  warn "'npm sbom' unavailable/failed for '$name'; falling back to 'npm ls --json'"
  local lstmp="$OUT_DIR/.npm-$name.ls.json.tmp"
  if ( cd "$dir" && npm ls --all --json ) > "$lstmp" 2>/dev/null && [[ -s "$lstmp" ]]; then
    mv -f "$lstmp" "$OUT_DIR/npm-$name.ls.json"
    info "wrote $OUT_DIR/npm-$name.ls.json (dependency tree; convert in CI)"
  else
    rm -f "$lstmp"
    warn "'npm ls' failed for '$name' (deps may not be installed) — no npm SBOM for it"
  fi
}

npm_sbom "vault-ui" "extensions/vault-ui"
npm_sbom "proxy-manager" "extensions/proxy-manager"
npm_sbom "ai-sidebar" "extensions/ai-sidebar"

# --- summary ----------------------------------------------------------------

info "SBOM generation finished. Files in $OUT_DIR:"
if command -v find >/dev/null 2>&1; then
  find "$OUT_DIR" -maxdepth 1 -type f -name '*.json' -printf '  %f\n' 2>/dev/null || true
fi
info "REMINDER: a release MUST publish a complete SBOM (§11). Re-run where cargo, npm,"
info "cargo-cyclonedx, and installed node_modules are all present, and treat warnings as gaps."
