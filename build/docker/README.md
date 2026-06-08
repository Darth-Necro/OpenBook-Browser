<!-- SPDX-License-Identifier: MPL-2.0 -->

# `build/docker/` — pinned build & reproducibility images (Build Plan §8/§9)

Containerized, **pinned-toolchain** images. Containers are how OpenBook gets a
deterministic toolchain across machines — the prerequisite for reproducible
builds (§8) and for the destructive-test isolation invariant (CLAUDE.md #7).

## Images

| File | Purpose |
|---|---|
| [`build.linux.Dockerfile`](build.linux.Dockerfile) | Linux x86_64 **build** image: pinned base (by digest), pinned Rust/clang/Node/Python, Gecko build deps. Used by `build/scripts/build.sh`. |
| [`repro.Dockerfile`](repro.Dockerfile) | **Reproducible-build** image: stricter than the build image — digest-pinned base, exact toolchain versions, `SOURCE_DATE_EPOCH` discipline — so a clean-room rebuild is byte-identical to the published artifact. |
| `vault-harness.Dockerfile` | Disposable harness for the vault native host's destructive tests (owned by the native/vault-host track — not documented in detail here). |

## Pinning policy (read before bumping)

- **Base image by digest.** Tags like `ubuntu:24.04` move; a moving base breaks
  determinism. `repro.Dockerfile` REQUIRES the digest form
  (`ubuntu@sha256:<digest>`); `build.linux.Dockerfile` ships a tag default with the
  digest form shown for switchover. Get the digest with:
  ```sh
  docker pull ubuntu:24.04
  docker inspect --format='{{index .RepoDigests 0}}' ubuntu:24.04
  ```
  Set it via `--build-arg BASE_IMAGE=ubuntu@sha256:...` or by editing the `ARG`.
- **Toolchain versions are the contract.** `RUST_VERSION`, `NODE_MAJOR`, and the
  apt package set are pinned. `RUST_VERSION` must satisfy both the crates'
  `rust-version` (>= 1.74) and upstream Firefox 145's minimum.
- **Firefox pin.** `repro.Dockerfile`'s `FIREFOX_PIN` (145.0.2) is kept in lockstep
  with `build/scripts/fetch-verify-upstream.sh` and `ci/mfsa-track.sh`.
- **Record every bump in `docs/DECISIONS.md`.**

## Building the images

```sh
# Build image (context is the repo root):
docker build -f build/docker/build.linux.Dockerfile -t openbook-build-linux .

# Reproducible image (set a verified base digest first):
docker build -f build/docker/repro.Dockerfile \
  --build-arg BASE_IMAGE=ubuntu@sha256:<verified-digest> \
  -t openbook-repro .
```

## Building OpenBook inside an image

Mount the repo and map your UID so output files are owned by you (the images ship
no secrets and no signing keys):

```sh
docker run --rm -u "$(id -u):$(id -g)" -v "$PWD:/src" -w /src \
  openbook-build-linux build/scripts/build.sh --target linux-x64
```

Firefox build requirements (firefox-source-docs): 64-bit host, **>= 30 GB free
disk**, **8 GB+ RAM**, Python 3.9+, the `mach` build system. A full (non-artifact)
build compiles for hours — ensure the Docker host has the headroom. Frontend-only
iteration can use artifact builds.

## Reproducible-build verification

The whole point of `repro.Dockerfile` is that a distrustful third party can
reproduce the release and diff it:

```sh
# Rebuild from the pinned, verified source with the SAME SOURCE_DATE_EPOCH.
docker run --rm -e SOURCE_DATE_EPOCH=<published-value> \
  -u "$(id -u):$(id -g)" -v "$PWD:/src" -w /src \
  openbook-repro bash -lc 'build/scripts/build.sh --target linux-x64 && \
                           build/scripts/package.sh --source <objdir> --target linux-x64 --format tar.xz'

# Diff the rebuild against the published artifact.
python3 tests/repro/repro_diff.py ./rebuilt.tar.xz ./published.tar.xz
# or:  ci/repro-diff.sh ./rebuilt.tar.xz ./published.tar.xz
```

`MATCH` (exit 0) proves the published binary corresponds to this source. See
[`tests/repro/README.md`](../../tests/repro/README.md) for the full procedure and
the v2 (Tor `rbm`-style) target.

## What these images deliberately do NOT contain

- **No signing keys, no secrets.** Signing happens in a separate, HSM-backed step
  (`build/scripts/sign.sh`); keys never enter an image or the repo (§11).
- **No telemetry / phone-home** in the build itself.
