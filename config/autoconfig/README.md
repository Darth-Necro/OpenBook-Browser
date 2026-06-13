<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook AutoConfig (preference hardening)

This directory holds the **core Phase 1 hardening artifact**: the Firefox
AutoConfig pair that ships OpenBook's privacy defaults and locks the
security-critical ones.

## The two files and where they go in a build

| File in repo | Installed path in the built product | Role |
|---|---|---|
| `autoconfig.js` | `<install>/defaults/pref/autoconfig.js` | Tells Firefox to read an external AutoConfig file and names it. |
| `openbook.cfg` | `<install>/openbook.cfg` (application **install root**) | The actual hardened preference script (privileged JS). |

The branding/packaging patches (`patches/branding`, `patches/features`) and the
packaging step copy these into place; see the per-platform packaging scripts in
`build/`.

## How AutoConfig works

1. On startup Firefox reads `defaults/pref/autoconfig.js`. Ours sets:
   - `general.config.filename = "openbook.cfg"` — the external config filename,
     resolved relative to the install root.
   - `general.config.obscure_value = 0` — historically the `.cfg` could be
     byte-rotated (ROT-13-ish) to discourage casual edits. `0` means **plain
     text**, which is what we want: the config must be auditable. Obscuring is
     not a security control (the permissions invariant is).
   - The AutoConfig **sandbox stays enabled** (the default). The sandbox
     provides `pref`/`defaultPref`/`lockPref`/`clearPref` — everything
     `openbook.cfg` uses — so disabling it (as some templates do) would grant
     the config full chrome privilege for no functional gain. Least privilege
     applies to our own config too (§11).
2. Firefox then loads and **executes `openbook.cfg` as privileged JavaScript**
   with chrome privileges, applying every `pref` / `defaultPref` / `lockPref`
   call.

## Gotchas this layer must respect (and the tests enforce)

- **The first line of `openbook.cfg` is ALWAYS IGNORED** by the AutoConfig
  parser. Line 1 must be a comment. Ours is
  `// OpenBook AutoConfig — do not remove or edit this first line.`
  If you put a real `pref()` on line 1 it silently does nothing.
- **Values must be real JS literals**, not strings. `lockPref("x", false)` is
  correct; `lockPref("x", "false")` sets the *string* `"false"`, which is
  truthy and a real, hard-to-spot bug. `test_autoconfig_hardening.py` fails the
  build if any boolean-looking pref is quoted.
- **It is privileged JavaScript.** A syntax error throws at startup and can stop
  the browser from launching. Keep it valid; the tests parse it.
- `lockPref` vs `defaultPref` vs `pref`:
  - `lockPref` — set default **and lock**; user cannot override. Use for
    security invariants (telemetry off, HTTPS-only, no speculative connections).
  - `defaultPref` — set the default; user may change it. Use where there is a
    real UX tradeoff (RFP, DoH resolver, search suggestions, geo, WebGL).
  - `pref` — set the active value once; used sparingly.

## Permissions invariant (release blocker)

Per `docs/OpenBook-Browser-Build-Plan.md` §4 and §11, `openbook.cfg` and
`defaults/pref/*.js` are privileged. **They must be installed root-owned and not
user-writable** (mode `0644`, in a non-user-writable directory). A user-writable
privileged-JS config is a local privilege-escalation / code-execution hole — the
same class of finding documented against misconfigured Firefox enterprise
deployments. Packaging asserts ownership and mode; the privacy-regression /
packaging tests check it. Treat any deviation as a release blocker.

## Validating syntax locally

The `.cfg` is JavaScript with the first line stripped. To smoke-test that it
parses as JS (Node or any JS engine):

```sh
# Strip the mandatory first comment line, wrap the AutoConfig API as no-ops,
# and let the JS engine parse/execute it. Exit 0 = parses cleanly.
{ echo 'function pref(){}; function defaultPref(){}; function lockPref(){}; \
        function clearPref(){}; function getPref(){};'; \
  tail -n +2 openbook.cfg; } | node --check /dev/stdin \
  || echo "openbook.cfg failed to parse"
```

`tests/privacy-regression/test_autoconfig_hardening.py` and
`test_autoconfig_js.py` provide the authoritative, dependency-free checks
(run with `python3`).
