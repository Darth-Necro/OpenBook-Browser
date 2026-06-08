#!/usr/bin/env bash
set -euo pipefail

cat <<'MESSAGE'
Signing is a Phase 5 release-engineering operation. This script intentionally has
no default signing behavior because keys must live in hardware-backed systems or
platform signing services, never in repository files or plaintext CI variables.
MESSAGE
exit 64
