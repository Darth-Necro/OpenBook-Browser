#!/usr/bin/env python3
"""Release-layer gate (Build Plan §8/§9, ADR-0017).

Statically validates the versioning + release-engineering layer so that drift
between the VERSION file, the upstream pin, the changelog, and the release
workflow is caught in CI long before a tag is cut.
"""
import re
import stat
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

VERSION_RE = re.compile(r"^(\d+(?:\.\d+)*)-(\d+)$")

CHECKS = 0


def check(condition: bool, message: str) -> None:
    global CHECKS
    CHECKS += 1
    assert condition, message


def read(relative: str) -> str:
    p = ROOT / relative
    assert p.is_file(), f"Missing file: {relative}"
    return p.read_text(encoding="utf-8")


def main() -> None:
    # --- VERSION: format and single-source-of-truth agreement ---------------
    version = read("VERSION").strip()
    m = VERSION_RE.match(version)
    check(m is not None, f"VERSION {version!r} is not <firefox-version>-<openbook-build>")
    upstream_part = m.group(1)

    fetch = read("build/scripts/fetch-verify-upstream.sh")
    pin = re.search(r'^FIREFOX_VERSION="([^"]+)"', fetch, re.MULTILINE)
    check(pin is not None, "fetch-verify-upstream.sh has no FIREFOX_VERSION pin")
    check(
        upstream_part == pin.group(1),
        f"VERSION prefix {upstream_part!r} != fetch-verify-upstream.sh pin {pin.group(1)!r}",
    )

    mfsa = read("ci/mfsa-track.sh")
    check(
        pin.group(1) in mfsa,
        f"ci/mfsa-track.sh does not reference the pinned version {pin.group(1)!r}",
    )

    # --- CHANGELOG covers the current version --------------------------------
    changelog = read("CHANGELOG.md")
    check(
        re.search(rf"^## {re.escape(version)}\b", changelog, re.MULTILINE) is not None,
        f"CHANGELOG.md has no '## {version}' section",
    )

    # --- release workflow exists and is wired to the right mechanisms -------
    workflow = read(".github/workflows/release.yml")
    for needle, why in [
        ('tags: ["v*"]', "release workflow must trigger on v* tags"),
        ("package-components.sh", "release workflow must assemble via package-components.sh"),
        ("--with-sbom", "release workflow must produce the strict release SBOM"),
        ("--draft", "release workflow must create a DRAFT (publish is manual, post-checklist)"),
        ("--verify-tag", "release workflow must refuse to release an unverified tag"),
        ("refs/tags/v${version}", "release workflow must fail hard on tag/VERSION mismatch"),
        ("tests/release/test_release_layer.py", "release workflow must run this gate"),
    ]:
        check(needle in workflow, f"release.yml: {why} (missing {needle!r})")
    check(
        "OPENBOOK_GPG_KEY_ID" not in workflow and "secrets.GPG" not in workflow,
        "release.yml must not sign in CI — keys live on maintainer hardware (§11)",
    )

    # --- packaging script: present, executable, syntactically valid ---------
    pkg_rel = "build/scripts/package-components.sh"
    pkg = read(pkg_rel)
    mode = (ROOT / pkg_rel).stat().st_mode
    check(bool(mode & stat.S_IXUSR), f"{pkg_rel} is not executable")
    subprocess.run(["bash", "-n", str(ROOT / pkg_rel)], check=True)
    for needle, why in [
        ("SOURCE_DATE_EPOCH", "deterministic timestamps are required (Build Plan §8)"),
        ("--locked", "native hosts must build with locked dependencies"),
        ("SHA256SUMS", "the artifact set must be checksummed"),
        ("die 4", "missing/unbuilt components must fail closed"),
        ("sign.sh", "the script must point at the separate signing step"),
    ]:
        check(needle in pkg, f"{pkg_rel}: {why} (missing {needle!r})")

    # --- release process docs -------------------------------------------------
    checklist = read("docs/RELEASE-CHECKLIST.md")
    for needle in [
        "verify-release-permissions.sh",
        "repro-diff.sh",
        "sign.sh",
        "mfsa-track.sh",
        "FIRST_RUN_EGRESS",
    ]:
        check(needle in checklist, f"RELEASE-CHECKLIST.md must reference {needle}")

    read("CONTRIBUTING.md")
    check(
        "VERSION" in read("ci/release-pipeline.md"),
        "ci/release-pipeline.md must document the VERSION source of truth",
    )

    print(f"OK: release layer verified (version {version}, {CHECKS} checks).")


if __name__ == "__main__":
    main()
