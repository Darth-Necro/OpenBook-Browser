<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook enterprise policies (`policies.json`)

This is the Mozilla **enterprise policy engine** layer. It is **defense in
depth**, deliberately duplicating several of the locks already set in
`config/autoconfig/openbook.cfg`. If a future Firefox release changes a pref
name, or an AutoConfig edge case fails to apply, the policy still enforces the
invariant (and vice-versa). Belt and suspenders is intentional here because
"zero telemetry" is a hard security invariant, not a preference.

## Where it ships

`policies.json` is installed at:

- Linux: `<install>/distribution/policies.json`
- Windows: `<install>\distribution\policies.json`
- macOS: `<App>.app/Contents/Resources/distribution/policies.json`

(See `config/distribution/` and the packaging scripts.) On enterprise-managed
systems the OS policy store can also supply it; OpenBook ships its own so the
defaults hold on every install.

## Policy vs. AutoConfig — division of labor

| Use **policy** for | Use **AutoConfig** for |
|---|---|
| Things the policy engine models as first-class controls: default search engine, extension install allow/blocklists, disabling whole features (Pocket, studies, telemetry), user-messaging/recommendation surfaces, DoH provider, permission defaults. | The long tail of `about:config` prefs with no policy equivalent, and any value that must be **locked** at the pref layer (speculative connections, HTTPS-only, beacon, prefetch, referrer policy, fingerprinting nuance). |

Rule of thumb: if Mozilla exposes a policy for it, set the policy **and** the
pref. Otherwise the pref (`.cfg`) is the only mechanism.

## What this file enforces (highlights)

- `DisableTelemetry: true`, `DisableFirefoxStudies: true` — telemetry/Shield off
  at the policy layer (mirrors the locked `.cfg` telemetry block).
- `DisablePocket: true` — Pocket off (mirrors locked `extensions.pocket.enabled`).
- `DisableDefaultBrowserAgent: true`, `DontCheckDefaultBrowser: true` — no
  default-browser nag/agent.
- `OverrideFirstRunPage: ""`, `OverridePostUpdatePage: ""`, `NoDefaultBookmarks:
  true` — no first-run marketing, no post-update tour, no bundled bookmarks.
- `Cookies.Behavior: "reject-tracker-and-partition-foreign"` (Locked: false) and
  `EnableTrackingProtection {Value: true, Cryptomining, Fingerprinting}`
  (Locked: false) — strong tracking defaults the user can still relax.
- `FirefoxHome` — sponsored top sites, Pocket, sponsored Pocket, snippets and
  highlights all off.
- `UserMessaging` (Locked: true) — extension/feature recommendations, urlbar
  interventions, "More from Mozilla" and What's New all off; onboarding skipped.
- `FirefoxSuggest` / `SearchSuggestEnabled: false` — no sponsored or remote
  suggestions; no per-keystroke suggestion egress by default.
- `SearchEngines.Default: "DuckDuckGo"` — a privacy-respecting default. **Users
  can change it** (`PreventInstalls: false`); this is a default, not a lock.
- `DNSOverHTTPS` — DoH on with a neutral provider and fallback; not locked.
- `ExtensionSettings` — the three first-party OpenBook extensions
  (`vault-ui@openbook.browser`, `proxy-manager@openbook.browser`,
  `ai-sidebar@openbook.browser`) are explicitly `allowed`; the `*` default is
  also `allowed` so users keep add-on freedom, with a trust-reminder install
  message.
- `Permissions` — Location requests blocked by default (matches `geo.enabled`
  off in the `.cfg`); Camera/Microphone/Notifications left to per-site prompts;
  Autoplay `block-audio-video`.
- `DisableAppUpdate: false` — **updates are kept on** (security patches matter).

## Validation

`policies.json` is **strictly valid JSON** — JSON has no comments, so this file
contains none. Every key/sub-key/enum used here was checked against the Mozilla
policy schema (`policies-schema.json`) and the
[mozilla/policy-templates](https://github.com/mozilla/policy-templates)
documentation. Notably, only real policies are used (e.g. there is no
`DisableTelemetryCoordinator` — that is not a policy).

Validate locally:

```sh
python3 -c "import json; json.load(open('config/policies/policies.json'))"
```

The authoritative, dependency-free check is
`tests/privacy-regression/test_policies_json.py` (`python3` to run).

CI additionally validates the file against the upstream
`policies-schema.json` of the pinned Firefox 145.0.2 on a real checkout, since
the schema can shift between Firefox versions.
