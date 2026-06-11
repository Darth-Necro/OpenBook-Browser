#!/usr/bin/env bash
# SPDX-License-Identifier: MPL-2.0
# Run the jest unit suites for all three bundled OpenBook WebExtensions.
#
# This runs the PURE-logic unit tests only (no browser, no native host, no
# network). Integration tests that need a real Firefox build (web-ext /
# Marionette / Playwright-Firefox) live elsewhere — see this directory's
# README.md and tests/leak/, tests/native/.
set -euo pipefail

# Resolve the repo root from this script's location so it runs from anywhere.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
EXT_DIR="${REPO_ROOT}/extensions"

EXTENSIONS=(vault-ui proxy-manager ai-sidebar)

echo "== OpenBook extension unit tests =="

# Ensure deps are present for each extension; install if missing.
for ext in "${EXTENSIONS[@]}"; do
  if [ ! -d "${EXT_DIR}/${ext}/node_modules" ]; then
    echo "-- installing deps for ${ext}"
    npm --prefix "${EXT_DIR}/${ext}" ci
  fi
done

# Type-check (build) then test each extension. tsc must pass with zero errors.
for ext in "${EXTENSIONS[@]}"; do
  echo "-- building ${ext}"
  npm --prefix "${EXT_DIR}/${ext}" run build
done

echo "-- testing vault-ui"
npm --prefix "${EXT_DIR}/vault-ui" test

echo "-- testing proxy-manager"
npm --prefix "${EXT_DIR}/proxy-manager" test

echo "-- testing ai-sidebar"
npm --prefix "${EXT_DIR}/ai-sidebar" test

echo "== all extension unit suites passed =="
