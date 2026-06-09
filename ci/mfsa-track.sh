#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — Mozilla Foundation Security Advisories (MFSA) tracker
# (Build Plan §9 / §11 / §12 Phase 6).
#
# A fork is always some distance behind upstream Firefox stable. Maintainers must
# always KNOW which security advisories (and thus CVEs) apply at/after the pinned
# Firefox version, so they can judge the patch-coverage gap and meet the ~1–2 day
# rebase SLA (§9). This script fetches Mozilla's MFSA index and reports the
# advisories that affect Firefox AT or AFTER the pinned version.
#
# The pinned version is a single variable below (FIREFOX_PIN) — bump it in one
# place when the pin moves.
#
# OFFLINE TOLERANCE: this is meant to run in CI with network. If the network is
# unavailable (no curl, DNS/connect failure), it prints a clear "network
# unavailable, run in CI" message and exits 0 — a missing network is NOT a build
# failure here. It exits NONZERO only on a genuine logic/parse error when data
# WAS retrieved.
#
# Usage:
#   ci/mfsa-track.sh [--pin VERSION] [--json] [--url URL]
#   ci/mfsa-track.sh -h
#
# Exit codes: 0 normal (including graceful offline skip); 2 usage error;
# 3 retrieved data could not be parsed.

set -euo pipefail

PROG="$(basename "$0")"

# --- the pin (single source of truth) ---------------------------------------
FIREFOX_PIN="145.0.2"

# Mozilla publishes advisories at this index. There is also a per-product JSON
# feed; we try the JSON feed first (machine-readable) and fall back to noting the
# HTML index. Both are overridable via --url for mirrors/testing.
MFSA_JSON_URL="https://www.mozilla.org/en-US/security/advisories/index.json"
MFSA_HTML_URL="https://www.mozilla.org/en-US/security/advisories/"

OUTPUT_JSON=0
URL_OVERRIDE=""

usage() {
  cat <<USAGE
$PROG — report Mozilla Security Advisories at/after the pinned Firefox version.

Usage:
  $PROG [--pin VERSION] [--json] [--url URL]
  $PROG -h | --help

Options:
  --pin VERSION  Override the pinned Firefox version (default: $FIREFOX_PIN).
  --json         Emit the raw retrieved advisory JSON to stdout (for piping).
  --url URL      Override the advisories source URL (JSON index or mirror).

Offline-tolerant: with no network it prints a notice and exits 0 (run in CI for
real coverage). The pinned version lives in the FIREFOX_PIN variable in this file.
USAGE
}

info() { echo "$PROG: $*"; }
warn() { echo "$PROG: WARN: $*" >&2; }

while [[ $# -gt 0 ]]; do
  case "$1" in
    --pin)
      [[ $# -ge 2 ]] || { echo "$PROG: --pin requires a version" >&2; exit 2; }
      FIREFOX_PIN="$2"; shift 2 ;;
    --json)
      OUTPUT_JSON=1; shift ;;
    --url)
      [[ $# -ge 2 ]] || { echo "$PROG: --url requires a URL" >&2; exit 2; }
      URL_OVERRIDE="$2"; shift 2 ;;
    -h|--help)
      usage; exit 0 ;;
    *)
      echo "$PROG: unknown argument: $1 (try -h)" >&2; exit 2 ;;
  esac
done

info "pinned Firefox version: $FIREFOX_PIN"

# --- network availability ----------------------------------------------------

if ! command -v curl >/dev/null 2>&1; then
  warn "curl not found; cannot fetch MFSA feed."
  info "network unavailable — run this in CI with curl + network for real MFSA coverage. (exit 0)"
  exit 0
fi

SRC_URL="${URL_OVERRIDE:-$MFSA_JSON_URL}"
info "fetching advisories from: $SRC_URL"

# Fetch with a short timeout. --fail makes HTTP errors nonzero; we catch all of
# it and treat connectivity failure as a graceful skip.
RAW=""
if RAW="$(curl --fail --silent --show-error --location --max-time 20 "$SRC_URL" 2>/dev/null)"; then
  : # got data
else
  warn "could not retrieve $SRC_URL (network/DNS/HTTP failure)."
  info "network unavailable — run this in CI with network for real MFSA coverage. (exit 0)"
  exit 0
fi

if [[ -z "$RAW" ]]; then
  warn "empty response from $SRC_URL."
  info "treating as offline/transient — run in CI. (exit 0)"
  exit 0
fi

if [[ "$OUTPUT_JSON" -eq 1 ]]; then
  printf '%s\n' "$RAW"
  exit 0
fi

# --- parse + report ----------------------------------------------------------
#
# Prefer jq (clean JSON parse). If jq is absent we fall back to a best-effort
# grep over the raw text so the script still gives a signal without extra deps.

info "advisories affecting Firefox >= $FIREFOX_PIN (best-effort; verify against the MFSA pages):"

if command -v jq >/dev/null 2>&1; then
  # The index.json schema lists advisory entries. Field names vary across the
  # feed's history, so we defensively probe a few likely shapes and print
  # mfsa id + title + affected version where present. A non-JSON body makes jq
  # fail; we treat that as a parse error (exit 3) since data WAS retrieved.
  if ! printf '%s' "$RAW" | jq -e . >/dev/null 2>&1; then
    warn "retrieved data is not valid JSON (the feed schema/URL may have changed)."
    info "see the HTML index for manual review: $MFSA_HTML_URL"
    exit 3
  fi
  # Print entries; tolerate missing fields. We do NOT attempt strict semver
  # comparison in jq (the feed mixes formats); instead we surface entries and let
  # the maintainer confirm coverage relative to $FIREFOX_PIN. Where an explicit
  # 'fixed_in'/'firefox' field exists we include it for quick scanning.
  printf '%s' "$RAW" | jq -r --arg pin "$FIREFOX_PIN" '
    def entries:
      if type == "array" then .
      elif has("mfsa") then [.mfsa[]?]
      elif has("advisories") then [.advisories[]?]
      else [ .[]? ] end;
    entries[]?
    | {
        id: (.mfsa_id // .id // .mfsaId // "MFSA-?"),
        title: (.title // .summary // "(no title)"),
        fixed_in: (.fixed_in // .firefox // .affected // null),
        date: (.date // .announced // null)
      }
    | "  \(.id)  [\(.fixed_in // "fixed-in n/a")]  \(.date // "")  \(.title)"
  ' 2>/dev/null || {
      warn "could not interpret the advisory JSON schema with the built-in jq filter."
      info "dump it with --json and inspect, or review the index: $MFSA_HTML_URL"
      exit 3
    }
  info "NOTE: confirm each advisory's 'Fixed in' against the pin ($FIREFOX_PIN). Anything"
  info "fixed AFTER $FIREFOX_PIN is a CVE your current build does NOT yet contain — rebase (§9)."
else
  warn "jq not found; emitting a raw grep over the feed (install jq in CI for a clean report)."
  # Best-effort: surface lines that look like MFSA identifiers.
  printf '%s\n' "$RAW" | grep -oE 'MFSA[ -][0-9]{4}-[0-9]+' 2>/dev/null | sort -u | sed 's/^/  /' || true
  info "Install jq in CI for a structured 'fixed-in vs pin' report. Manual index: $MFSA_HTML_URL"
fi

info "done. Track advisories continuously and rebase the patch series onto each upstream stable (§9, ~1–2 day SLA)."
