# Build Guide

## Phase 0 quick checks

```bash
python3 tests/phase0/test_phase0_structure.py
find build/scripts -type f -name '*.sh' -print0 | xargs -0 -n1 bash -n
```

## Fetch and verify upstream Firefox source

The upstream source version is pinned inside `build/scripts/fetch-verify-upstream.sh`.

```bash
OPENBOOK_UPSTREAM_GPG_KEYRING=/path/to/mozilla-release-signing.gpg \
  build/scripts/fetch-verify-upstream.sh --dest /tmp/openbook-upstream
```

The script downloads the source tarball, Mozilla SHA256 manifest, and detached signatures, then fails hard if manifest signature verification or the source tarball checksum fails.

## Apply patches

```bash
build/scripts/apply-patches.sh --source /tmp/openbook-upstream/firefox-145.0.2
```

Phase 0 has no patches, so this command is a no-op with a success message.

## Invoke a Firefox build

A full Firefox build is intentionally not run casually. On a suitable build host:

```bash
build/scripts/build.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target linux-x64
build/scripts/build.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target win-x64
build/scripts/build.sh --source /tmp/openbook-upstream/firefox-145.0.2 --target macos-universal
```

`build.sh` automatically calls `install-config.sh` at the end of the build to copy the AutoConfig loader, locked config, and enterprise policies into the Firefox dist directory.

Use `--artifact` for frontend-only iteration where supported.

## Phase 1 quick checks

```bash
python3 tests/phase1/test_phase1_config.py
python3 -c "import json; json.load(open('config/distribution/policies.json'))"
head -1 config/autoconfig/openbook.cfg   # must start with //
```

## Install config files into an existing dist directory

If you have an existing Firefox build and want to apply OpenBook config files:

```bash
build/scripts/install-config.sh --dist /path/to/dist/bin
```
