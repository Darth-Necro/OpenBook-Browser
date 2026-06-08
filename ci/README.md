<!-- SPDX-License-Identifier: MPL-2.0 -->

# `ci/` — pipelines and release gates

Index of the CI scripts and what each enforces. The full end-to-end flow,
build matrix, patch-maintenance/MFSA SLA, and update-distribution choice are in
[`release-pipeline.md`](release-pipeline.md).

| Script / doc | Purpose | Gate it enforces |
|---|---|---|
| [`sbom.sh`](sbom.sh) | Generate a CycloneDX SBOM aggregating Rust (`native/vault-host`, `native/vpn-helper`) and npm (`extensions/*`) dependencies into `sbom/`. | **Supply chain (§11):** a release must publish a complete SBOM. Degrades gracefully (warns, continues) when a tool or component is missing; treat warnings as gaps on a real release. |
| [`repro-diff.sh`](repro-diff.sh) | Thin wrapper over `tests/repro/repro_diff.py`. | **Reproducible builds (§5/§10):** a clean-room rebuild must be byte-identical to the published artifact. Exit 0 MATCH, 1 MISMATCH, 2 usage. |
| [`mfsa-track.sh`](mfsa-track.sh) | Fetch Mozilla Foundation Security Advisories and report those affecting Firefox **≥ the pin (145.0.2)**. | **Upstream security tracking (§9):** maintainers always know the CVE coverage gap and meet the ~1–2 day rebase SLA. **Offline-tolerant:** prints a notice and exits 0 with no network. |
| [`release-pipeline.md`](release-pipeline.md) | The authoritative §9 pipeline doc: fetch→verify(fail-hard)→patch(fail-on-conflict)→matrix build→tests→package→sign→publish, plus matrix, SLA, and update strategy. | Documentation of the trust-critical fail-hard points. |

## Related scripts outside `ci/`

These are invoked by the pipeline but live with the build/test layers:

- `build/scripts/fetch-verify-upstream.sh` — pin + **fail-hard** source verification.
- `build/scripts/apply-patches.sh` — ordered patch series, **fail on conflict**.
- `build/scripts/build.sh` — `mach` build wrapper, per target.
- `build/scripts/package.sh` — per-OS packaging, **fails closed** on missing tools.
- `build/scripts/sign.sh` — per-OS signing; **keys only in HSM**, refuses if unset.
- `tests/leak/` — proxy/VPN leak gates (`failclosed_sim.py`, `leak_assertions.py`).
- `tests/repro/` — reproducible-build diff (`repro_diff.py`, `test_repro_diff.py`).

## Invariants these gates protect

- **Zero telemetry / no unexpected egress** — enforced by the privacy-regression
  and first-run egress tests (run in the pipeline; see `release-pipeline.md`).
- **Fail-closed proxy/VPN** — `tests/leak/failclosed_sim.py` (logic) +
  `leak_assertions.py` (config) + the live harness.
- **Signing keys never in repo/CI plaintext** — `build/scripts/sign.sh` references
  HSM/hardware-token handles only and fails closed when absent.
- **Reproducibility** — `ci/repro-diff.sh` + the pinned containers in `build/docker/`.
- **Supply chain** — `ci/sbom.sh` per release.
- **Upstream CVE coverage** — `ci/mfsa-track.sh`.
