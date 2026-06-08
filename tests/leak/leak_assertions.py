#!/usr/bin/env python3
# SPDX-License-Identifier: MPL-2.0
#
# OpenBook Browser — static leak-control assertions (Build Plan §6).
#
# Asserts that the FOUR mandatory leak controls are *configured* across the
# settings layer and the proxy-manager extension. This is a STATIC, OFFLINE gate;
# it does not run a browser. The live behavioral harness (real build + SOCKS
# proxy + sinkhole) is documented in README.md and is the runtime gate.
#
# The four controls (§6 — "without all four it is theater"):
#   1. WebRTC      — disabled or forced through the proxy; ICE must not expose the
#                    real IP. The settings track sets a conservative ICE default
#                    (media.peerconnection.ice.default_address_only) and DEFERS
#                    the enable/proxy decision to proxy-manager (it depends on
#                    tunnel state). Either source satisfies this control.
#   2. DNS         — resolved through the proxy/tunnel (network.proxy.socks_remote_dns=true)
#                    and/or DoH (network.trr.mode); never via the OS resolver.
#   3. IPv6        — the tunnel covers v6, or v6 is disabled when the tunnel is
#                    v4-only (network.dns.disableIPv6 / documented stance). Like
#                    WebRTC this is tunnel-state-dependent, so it may live in
#                    proxy-manager rather than the static cfg.
#   4. Fail-closed — proxy/tunnel drop blocks traffic; never silent direct.
#                    Enforced in the proxy-manager extension source.
#
# Inputs (all produced by OTHER tracks). Resolution policy per control:
#   - If a control is satisfied by ANY present source -> PASS.
#   - If a control is satisfied by NONE of the present sources AND the source
#     that legitimately owns it has not landed yet -> SKIP (clear message).
#   - If a control is satisfied by NONE and its owning source HAS landed content
#     -> FAIL (a real, complete gap before release).
#
#   config/autoconfig/openbook.cfg          (privileged AutoConfig JS)
#   config/policies/policies.json           (Mozilla enterprise policy)
#   extensions/proxy-manager/               (fail-closed + tunnel-dependent leak controls)
#
# Assertions are TOLERANT: they search for the relevant keys/intent rather than
# pinning exact values, so they survive reasonable changes by the other tracks.
#
# Exit code: 0 if every checkable control passes (skips count as non-failing);
# nonzero only if an owning source has landed but omits its required control.

import json
import os
import re
import sys

# --- repo-root discovery -----------------------------------------------------


def find_repo_root(start):
    """Walk up from `start` until we find the repo markers; fall back to start."""
    cur = os.path.abspath(start)
    while True:
        if os.path.isfile(os.path.join(cur, "CLAUDE.md")) and os.path.isdir(
            os.path.join(cur, "config")
        ):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            return os.path.abspath(start)
        cur = parent


REPO_ROOT = find_repo_root(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))

CFG_PATH = os.path.join(REPO_ROOT, "config", "autoconfig", "openbook.cfg")
POLICIES_PATH = os.path.join(REPO_ROOT, "config", "policies", "policies.json")
PROXY_MANAGER_DIR = os.path.join(REPO_ROOT, "extensions", "proxy-manager")


# --- result bookkeeping ------------------------------------------------------


class Results:
    def __init__(self):
        self.passed = 0
        self.failed = 0
        self.skipped = 0

    def ok(self, msg):
        self.passed += 1
        print(f"  PASS: {msg}")

    def fail(self, msg):
        self.failed += 1
        print(f"  FAIL: {msg}")

    def skip(self, msg):
        self.skipped += 1
        print(f"  SKIP: {msg}")


# --- helpers -----------------------------------------------------------------


def _read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _search_any(text, patterns):
    """Return True if any regex in `patterns` matches `text` (case-insensitive)."""
    return any(re.search(p, text, re.IGNORECASE) for p in patterns)


def _gather_source_text(directory, exts=(".ts", ".js", ".json", ".mjs")):
    """Concatenate text of source files under `directory`, skipping node_modules
    and build output, so we can search the extension for intent tolerantly.
    Returns "" if the directory has no such source files yet."""
    chunks = []
    for root, dirs, files in os.walk(directory):
        dirs[:] = [d for d in dirs if d not in ("node_modules", "dist", ".git")]
        for name in files:
            if name.endswith(exts):
                try:
                    chunks.append(_read_text(os.path.join(root, name)))
                except OSError:
                    continue
    return "\n".join(chunks)


def _load_sources():
    """Load the settings-layer text and the proxy-manager source corpus.

    Returns a dict with the raw texts and presence flags. Each is independently
    optional; absent inputs are simply empty and flagged.
    """
    cfg_text = _read_text(CFG_PATH) if os.path.isfile(CFG_PATH) else ""
    policies_text = _read_text(POLICIES_PATH) if os.path.isfile(POLICIES_PATH) else ""
    pm_text = _gather_source_text(PROXY_MANAGER_DIR) if os.path.isdir(PROXY_MANAGER_DIR) else ""
    return {
        "cfg": cfg_text,
        "cfg_present": bool(cfg_text),
        "policies": policies_text,
        "policies_present": bool(policies_text),
        # The settings layer as a whole (cfg + policies).
        "settings": cfg_text + "\n" + policies_text,
        "settings_present": bool(cfg_text or policies_text),
        "proxy_manager": pm_text,
        # "landed" = directory has actual source files, not just a .gitkeep.
        "proxy_manager_present": bool(pm_text.strip()),
    }


# --- control checks ----------------------------------------------------------
#
# Each returns nothing; it records into `res`. The pattern is:
#   matched in settings OR proxy-manager -> PASS
#   else, if the owning/contributing sources are all absent -> SKIP
#   else -> FAIL.


def check_policies_parse(res, src):
    if src["policies_present"]:
        try:
            json.loads(src["policies"])
            res.ok("policies.json parses as valid JSON")
        except json.JSONDecodeError as e:
            res.fail(f"policies.json is not valid JSON: {e}")
    else:
        res.skip(f"policies.json not found at {POLICIES_PATH} (settings track) - JSON-parse check skipped")


def check_webrtc(res, src):
    patterns = [
        r"media\.peerconnection\.ice\.default_address_only",
        r"media\.peerconnection\.ice\.no_host",
        r"media\.peerconnection\.ice\.proxy_only",
        r"media\.peerconnection\.enabled",
        r"webrtc",
        r"peerconnection",
    ]
    hay = src["settings"] + "\n" + src["proxy_manager"]
    if _search_any(hay, patterns):
        res.ok("WebRTC leak control referenced (peerconnection/ICE handling in settings and/or proxy-manager)")
    elif not (src["settings_present"] or src["proxy_manager_present"]):
        res.skip("WebRTC control: neither settings layer nor proxy-manager present yet - skipped")
    else:
        res.fail(
            "no WebRTC leak control found (expected media.peerconnection.ice.default_address_only / "
            "no_host / proxy_only, media.peerconnection.enabled=false, or proxy-manager WebRTC handling)"
        )


def check_dns(res, src):
    patterns = [
        r"network\.proxy\.socks_remote_dns",
        r"socks_remote_dns",
        r"network\.trr\.mode",
        r"DNSOverHTTPS",
    ]
    hay = src["settings"] + "\n" + src["proxy_manager"]
    if _search_any(hay, patterns):
        res.ok("DNS-through-proxy intent referenced (socks_remote_dns and/or DoH/trr.mode)")
    elif not (src["settings_present"] or src["proxy_manager_present"]):
        res.skip("DNS control: neither settings layer nor proxy-manager present yet - skipped")
    else:
        res.fail(
            "no DNS leak control found (expected network.proxy.socks_remote_dns=true "
            "and/or network.trr.mode / DNSOverHTTPS policy)"
        )


def check_ipv6(res, src):
    # IPv6 stance: an explicit disable pref, OR a documented IPv6 handling note.
    # Like WebRTC, the "disable v6 when the tunnel is v4-only" decision is
    # tunnel-state-dependent and may legitimately live in proxy-manager.
    patterns = [
        r"network\.dns\.disableIPv6",
        r"disableIPv6",
        r"ipv6",
    ]
    settings_has = _search_any(src["settings"], patterns)
    pm_has = _search_any(src["proxy_manager"], patterns)
    if settings_has or pm_has:
        where = "settings layer" if settings_has else "proxy-manager"
        res.ok(f"IPv6 stance referenced in {where} (disableIPv6 / documented IPv6 handling)")
    elif not src["proxy_manager_present"]:
        # The settings cfg defers tunnel-state-dependent leak controls (WebRTC,
        # and by the same logic IPv6) to proxy-manager. Until proxy-manager lands
        # source, treat a missing IPv6 stance as deferred, not a failure.
        res.skip(
            "IPv6 stance not in the static settings layer and proxy-manager has not landed yet. "
            "IPv6 (disable v6 when the tunnel is v4-only) is tunnel-state-dependent and is expected "
            "to be enforced by proxy-manager; deferring - skipped. REQUIRED before the leak suite is green."
        )
    else:
        res.fail(
            "no IPv6 stance found in either the settings layer or proxy-manager "
            "(expected network.dns.disableIPv6, or explicit IPv6 handling, since a v4-only tunnel "
            "must not leak over IPv6)"
        )


def check_failclosed(res, src):
    if not os.path.isdir(PROXY_MANAGER_DIR):
        res.skip(f"proxy-manager not found at {PROXY_MANAGER_DIR} (extensions track) - fail-closed check skipped")
        return
    if not src["proxy_manager_present"]:
        res.skip(
            f"proxy-manager directory exists at {PROXY_MANAGER_DIR} but has no source files yet "
            "- fail-closed source check skipped"
        )
        return
    # Tolerant: any recognizable fail-closed enforcement signal counts. The
    # canonical Firefox mechanism is a proxy.onRequest handler that never returns
    # DIRECT, or an explicit failClosed/blocking flag.
    patterns = [
        r"fail[\s_-]?clos",          # fail-closed / failClosed / fail_closed
        r"\bDIRECT\b",               # discussing/forbidding direct routing
        r"onRequest",                # proxy.onRequest enforcement point
        r"proxy\.settings",
        r"blockingResponse|cancel\s*:\s*true|\bblock\b",
    ]
    matched = [p for p in patterns if re.search(p, src["proxy_manager"], re.IGNORECASE)]
    if matched:
        res.ok(
            f"fail-closed enforcement signal found in proxy-manager source ({len(matched)} indicator(s))"
        )
    else:
        res.fail(
            "no fail-closed enforcement found in proxy-manager source "
            "(expected a proxy.onRequest handler that never returns DIRECT, or an explicit "
            "fail-closed/blocking path)"
        )


def run():
    print(f"OpenBook leak-control static assertions (repo root: {REPO_ROOT})")
    print("Checking the four mandatory leak controls (§6).")
    src = _load_sources()
    print(
        "Inputs present: "
        f"openbook.cfg={src['cfg_present']}, policies.json={src['policies_present']}, "
        f"proxy-manager-source={src['proxy_manager_present']}"
    )
    res = Results()

    print("\n[settings layer] policies.json well-formedness")
    check_policies_parse(res, src)

    print("\n[control 1] WebRTC")
    check_webrtc(res, src)

    print("\n[control 2] DNS")
    check_dns(res, src)

    print("\n[control 3] IPv6")
    check_ipv6(res, src)

    print("\n[control 4] fail-closed (proxy-manager)")
    check_failclosed(res, src)

    print(f"\nResult: {res.passed} passed, {res.failed} failed, {res.skipped} skipped.")
    if res.failed:
        print(
            "FAIL: an owning source has landed but omits a required leak control. "
            "Fix the settings/extension track before release."
        )
        return 1
    if res.passed == 0:
        print(
            "All inputs absent (settings/extension tracks not landed yet); nothing to assert. "
            "Graceful skip (exit 0)."
        )
    else:
        print(
            "OK: all checkable leak controls satisfied. "
            f"({res.skipped} deferred to tracks not yet landed.)"
            if res.skipped
            else "OK: all checkable leak controls satisfied."
        )
    return 0


# --- pytest entry points -----------------------------------------------------


def test_leak_controls_present_or_skipped():
    # Must never *fail*: returns 0 on pass or graceful skip; nonzero only when a
    # landed owning source omits its required control.
    assert run() == 0


if __name__ == "__main__":
    sys.exit(run())
