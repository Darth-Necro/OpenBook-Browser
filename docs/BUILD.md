# Build Guide

OpenBook is a patch + settings + extension + native-host layer over a pinned, verified upstream
Firefox source tarball. Most of this guide runs anywhere; **building actual Firefox** needs a build
host with ≥30 GB free disk (≥40 GB on Windows), 8 GB+ RAM, Python 3.9+, and the `mach` toolchain, and
is orchestrated by scripts/CI — not run inline. For frontend-only iteration use **artifact builds**.

## Offline gates (run anywhere, no Firefox checkout)

```bash
# Repo structure + shell syntax
python3 tests/phase0/test_phase0_structure.py
find build/scripts -type f -name '*.sh' -print0 | xargs -0 -n1 bash -n

# Phase 1 — privacy hardening assertions
python3 tests/privacy-regression/test_autoconfig_hardening.py
python3 tests/privacy-regression/test_autoconfig_js.py
python3 tests/privacy-regression/test_policies_json.py
python3 tests/privacy-regression/test_no_telemetry_endpoints.py

# Phase 3 — fail-closed / leak logic
python3 tests/leak/failclosed_sim.py
python3 tests/leak/leak_assertions.py

# Phase 5 — reproducible-build diff harness self-test
python3 tests/repro/test_repro_diff.py
```

## Native hosts (Rust)

The vault host's default feature set compiles **without** system TPM libraries (the `tpm` and
`secure-enclave` providers are feature-gated and off by default; the default build uses the software
fallback).

```bash
# Vault host (Phase 2)
cargo build --manifest-path native/vault-host/Cargo.toml
cargo test  --manifest-path native/vault-host/Cargo.toml
# or:
tests/native/run.sh

# With hardware TPM 2.0 (needs tpm2-tss / libtss2 on the host):
cargo test --manifest-path native/vault-host/Cargo.toml --features tpm

# VPN-helper (Phase 3 — verification-only scaffold)
cargo build --manifest-path native/vpn-helper/Cargo.toml
cargo test  --manifest-path native/vpn-helper/Cargo.toml
```

Destructive lockout/erasure tests run only inside the disposable harness:

```bash
docker build -f build/docker/vault-harness.Dockerfile -t openbook-vault-harness .
docker run --rm openbook-vault-harness   # runs cargo test against throwaway data only
```

## Extensions (TypeScript)

```bash
for ext in vault-ui proxy-manager ai-sidebar; do
  npm --prefix "extensions/$ext" ci
  npm --prefix "extensions/$ext" run build   # tsc, strict
  npm --prefix "extensions/$ext" test        # jest
done
# or:
tests/extensions/run.sh
```

## Settings layer (Phase 1)

`config/autoconfig/openbook.cfg` is privileged JavaScript installed at the browser's install root;
`config/autoconfig/autoconfig.js` is installed into `defaults/pref/`. Gotchas (see
`config/autoconfig/README.md`): the **first line of `openbook.cfg` is always ignored** and must be a
comment; values must be **real JS literals** (`false`, not `"false"`). `config/policies/policies.json`
is the enterprise-policy layer (validate against `github.com/mozilla/policy-templates`).

The privacy-regression suite (above) statically asserts these are hardened. The **first-run egress
test** that fails on any unexpected outbound connection runs in CI against a real build — see
`tests/privacy-regression/FIRST_RUN_EGRESS.md`.

## Fetch and verify upstream Firefox source

The pinned version (145.0.2) lives in `build/scripts/fetch-verify-upstream.sh`.

```bash
OPENBOOK_UPSTREAM_GPG_KEYRING=/path/to/mozilla-release-signing.gpg \
  build/scripts/fetch-verify-upstream.sh --dest /tmp/openbook-upstream
```

It downloads the source tarball, the Mozilla `SHA256SUMS` manifest, and its detached signature, then
**fails hard** if the manifest signature or the source checksum does not verify.

## Apply patches

```bash
build/scripts/apply-patches.sh --source /tmp/openbook-upstream/firefox-145.0.2
```

Ordered series under `patches/{branding,privacy,features}` applied sorted by path; **fails on the
first conflict**. The patches target firefox-145.0.2 and are rebased in CI on each upstream stable.

## Build Firefox (on a suitable build host)

```bash
build/scripts/build.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target linux-x64
build/scripts/build.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target win-x64
build/scripts/build.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target macos-universal
# Frontend-only iteration (downloads prebuilt internals, skips the C++ compile):
build/scripts/build.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target linux-x64 --artifact
```

`build.sh` stages `branding/openbook/` into `browser/branding/openbook/` before the build and, after
it, installs the OpenBook settings layer into the dist via `install-config.sh` (AutoConfig +
`policies.json`). It **fails closed** if branding, the dist, or the config cannot be installed — a build
never reports success without the hardening layer. `--skip-config-install` is for local development only
and its output must not be released. To install the config into a dist manually:

```bash
build/scripts/install-config.sh --dist <objdir>/dist/bin                         # Linux/Windows
build/scripts/install-config.sh --dist <objdir>/dist/*.app/Contents/Resources    # macOS
```

## Package, sign, SBOM, reproducibility (Phase 5)

```bash
build/scripts/package.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target linux-x64 --format deb
build/scripts/verify-release-permissions.sh --root <staged-package-root>     # §11 root-owned check
build/scripts/sign.sh    --target linux-x64 --artifact dist/openbook_*.deb   # keys from HSM/env only
ci/sbom.sh                                                                   # CycloneDX SBOM
ci/repro-diff.sh DIR_A DIR_B                                                 # rebuild-and-diff
ci/mfsa-track.sh                                                             # CVE coverage vs the pin
```

`package.sh` and `sign.sh` **fail closed** when a required packager or signing-key handle is missing —
they never emit a half-signed or partial artifact. Signing keys live only in HSM/hardware tokens or
platform signing services, never in the repo or CI plaintext (see `docs/SECURITY.md`).
