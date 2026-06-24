#!/usr/bin/env python3
from pathlib import Path
import stat

ROOT = Path(__file__).resolve().parents[2]

REQUIRED_DIRS = [
    "build/mozconfig",
    "build/scripts",
    "build/docker",
    "patches/branding",
    "patches/privacy",
    "patches/features",
    "branding",
    "config/autoconfig",
    "config/policies",
    "config/distribution",
    "extensions/vault-ui",
    "extensions/proxy-manager",
    "extensions/ai-sidebar",
    "native/vault-host",
    "native/vpn-helper",
    "ci",
    "tests/native",
    "tests/extensions",
    "tests/privacy-regression",
    "tests/leak",
    "tests/repro",
    "docs",
]

REQUIRED_FILES = [
    "CLAUDE.md",
    "docs/OpenBook-Browser-Build-Plan.md",
    "docs/DECISIONS.md",
    "docs/THREAT-MODEL.md",
    "docs/BUILD.md",
    "docs/SECURITY.md",
    "build/scripts/fetch-verify-upstream.sh",
    "build/scripts/apply-patches.sh",
    "build/scripts/build.sh",
    "build/scripts/package.sh",
    "build/scripts/sign.sh",
    "build/mozconfig/mozconfig.linux-x64",
    "build/mozconfig/mozconfig.win-x64",
    "build/mozconfig/mozconfig.macos-universal",
    "build/mozconfig/mozconfig.artifact",
    ".github/workflows/phase0.yml",
]


def assert_exists(relative_path: str, directory: bool = False) -> None:
    path = ROOT / relative_path
    if directory:
        assert path.is_dir(), f"missing directory: {relative_path}"
    else:
        assert path.is_file(), f"missing file: {relative_path}"


def assert_executable(relative_path: str) -> None:
    path = ROOT / relative_path
    mode = path.stat().st_mode
    assert mode & stat.S_IXUSR, f"script is not user-executable: {relative_path}"


def assert_contains(relative_path: str, text: str) -> None:
    content = (ROOT / relative_path).read_text(encoding="utf-8")
    assert text in content, f"{relative_path} does not contain {text!r}"


def main() -> None:
    for directory in REQUIRED_DIRS:
        assert_exists(directory, directory=True)
    for required_file in REQUIRED_FILES:
        assert_exists(required_file)
    for script in REQUIRED_FILES:
        if script.startswith("build/scripts/"):
            assert_executable(script)

    assert_contains("build/scripts/fetch-verify-upstream.sh", 'FIREFOX_VERSION="145.0.2"')
    assert_contains("build/scripts/fetch-verify-upstream.sh", "EXPECTED_KEY_FPRS")
    assert_contains("build/scripts/fetch-verify-upstream.sh", "VALIDSIG")
    assert_contains("build/scripts/fetch-verify-upstream.sh", "sha256sum --check --strict")
    assert_contains(".github/workflows/phase0.yml", "linux-x64")
    assert_contains(".github/workflows/phase0.yml", "win-x64")
    assert_contains(".github/workflows/phase0.yml", "macos-universal")
    assert_contains(".github/workflows/phase0.yml", "permissions:")


if __name__ == "__main__":
    main()
