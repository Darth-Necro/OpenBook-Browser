<!-- SPDX-License-Identifier: MPL-2.0 -->

# `tests/repro/` — reproducible-build diff (Build Plan §5/§8)

Reproducible builds are the mechanism that lets a **distrustful user verify the
binary matches the source** — the same trust model Tor Browser and LibreWolf use.
Instead of "trust our build server," anyone can rebuild OpenBook from the pinned
source in a clean, pinned environment and confirm the result is **byte-identical**
to the published artifact.

## Files

- **`repro_diff.py`** — compares two build outputs (two directories, or two files)
  by a **normalized SHA-256 manifest** and reports `MATCH` / `MISMATCH`.
  - Directory walks are **sorted** (traversal order never affects the result).
  - Paths are compared **relative** to each root with `/` separators, so the two
    roots may sit at different absolute locations.
  - **Symlinks** are recorded by target (not followed), so a link-vs-file swap or
    a retargeted link is detected.
  - Exit codes: **0** = MATCH, **1** = MISMATCH, **2** = usage/input error.
  - Usage:
    ```sh
    python3 tests/repro/repro_diff.py REBUILD_DIR PUBLISHED_DIR
    python3 tests/repro/repro_diff.py rebuilt.tar.xz published.tar.xz
    python3 tests/repro/repro_diff.py --manifest-only DIR   # print a manifest
    ```
- **`test_repro_diff.py`** — runnable test (`python3 tests/repro/test_repro_diff.py`,
  also pytest-discoverable). Builds two byte-identical trees and asserts MATCH,
  then mutates a byte / adds / removes a file and asserts MISMATCH with a nonzero
  exit, and checks that traversal order and root location do not affect the result.

## The reproducible-build procedure (§8)

A build is reproducible when the **same source** + the **same toolchain** + the
**same build inputs** always yield the **same bytes**. OpenBook's approach:

1. **Pinned, containerized toolchain.** Build inside the pinned image
   (`build/docker/repro.Dockerfile` — pinned base image digest, pinned Rust/clang/
   Node, pinned `mach` bootstrap). No "latest" anywhere. See `build/docker/README.md`.
2. **Pinned, verified source.** `build/scripts/fetch-verify-upstream.sh` pins the
   exact Firefox stable release (**145.0.2**) and verifies Mozilla's SHA-256
   manifest + detached signature before anything is built. The OpenBook patch
   series is applied deterministically (`apply-patches.sh`, fail-on-conflict).
3. **`SOURCE_DATE_EPOCH`.** Export a fixed `SOURCE_DATE_EPOCH` (e.g. the upstream
   release commit time) so every timestamp baked into the build and the packaging
   step is deterministic rather than "now."
4. **Deterministic packaging.** Archive/package steps sort entries and normalize
   timestamps, uid/gid, and permissions so the *container* of the bytes is
   reproducible too (not just the payload). `build/scripts/package.sh` performs the
   per-OS packaging on the proper build hosts.
5. **Publish hashes + signatures + SBOM.** Each release publishes SHA-256 sums,
   detached signatures (`build/scripts/sign.sh`, keys in HSM only), and a
   CycloneDX SBOM (`ci/sbom.sh`).

## How a distrustful user verifies

```sh
# 1. Build the published image from the pinned Dockerfile.
docker build -f build/docker/repro.Dockerfile -t openbook-repro .

# 2. Rebuild OpenBook from the pinned, verified source inside the container,
#    exporting the same SOURCE_DATE_EPOCH the release used.
docker run --rm -e SOURCE_DATE_EPOCH=<published-value> \
  -v "$PWD/out:/out" openbook-repro

# 3. Download the published artifact and diff it against the local rebuild.
python3 tests/repro/repro_diff.py ./out/openbook-145.0.2-linux-x64 ./published/openbook-145.0.2-linux-x64
#   MATCH   -> exit 0: the published binary corresponds to this source.
#   MISMATCH-> exit 1: investigate (toolchain drift, non-determinism, tampering).
```

A convenience wrapper is provided at `ci/repro-diff.sh`.

## v2 target

Align with **Tor's `rbm`-style** fully reproducible pipeline (deterministic,
hermetic, multi-arch, with reproducibility tracked per artifact in CI). The
current tooling proves the *diff* mechanism and the per-tree determinism; the v2
work is hardening the *build* itself to be bit-for-bit across independent
rebuilders.

## Note

`repro_diff.py` compares already-produced trees/files; it does not itself make a
build deterministic. Determinism comes from the pinned toolchain +
`SOURCE_DATE_EPOCH` + deterministic packaging above. This tool is the **verifier**.
