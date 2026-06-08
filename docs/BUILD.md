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

Use `--artifact` for frontend-only iteration where supported.
