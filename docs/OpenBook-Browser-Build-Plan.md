# OpenBook Browser — Build Plan

## Approach

OpenBook is a Firefox stable fork built as a patch, branding, settings, WebExtension, native-host, and release-engineering layer over a pinned and verified upstream Firefox source tarball.

## Repository layout

```text
build/      mozconfig/ scripts/ docker/
patches/    branding/ privacy/ features/
branding/   icons, names, strings
config/     autoconfig/ policies/ distribution/
extensions/ vault-ui/ proxy-manager/ ai-sidebar/
native/     vault-host/ vpn-helper/
ci/         pipelines, matrix, signing, SBOM, repro-diff
tests/      native/ extensions/ privacy-regression/ leak/ repro/ phase0/
docs/       OpenBook-Browser-Build-Plan.md THREAT-MODEL.md BUILD.md SECURITY.md DECISIONS.md
```

## Phase 0 — Foundations

Deliverables:

- Repository tree.
- `build/scripts/fetch-verify-upstream.sh` with an exact Firefox stable pin.
- `build/scripts/apply-patches.sh`.
- Per-platform mozconfigs.
- CI skeleton for Linux, Windows, and macOS.
- `THREAT-MODEL.md` v0.
- Governance and funding decision in `DECISIONS.md`.

Acceptance gate:

- The pipeline can fetch, verify, optionally patch, and invoke an unmodified Firefox build for Linux x64, Windows x64, and macOS universal targets.
- Local Phase 0 static checks pass.
- Do not begin branding or hardening until the Phase 0 gate is proven on suitable build hosts.

## Later phases

Phase 1 rebrands and hardens with AutoConfig and policies. Phase 2 implements cryptographic lockout only after a disposable VM/container harness exists. Phase 3 implements fail-closed proxy/VPN controls. Phase 4 implements the opt-in AI sidebar. Phase 5 completes signing, packaging, SBOM, and reproducible-build tooling. Phase 6 completes audit, disclosure, key-management, MFSA tracking, and sustainability work.
