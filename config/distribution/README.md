<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook distribution directory

Firefox reads a `distribution/` directory inside the application install root.
It is the home of two things OpenBook uses:

- `distribution.ini` — identifies the build as an OpenBook *distribution*
  (`id=openbook`, `version`, `about=OpenBook`). Firefox surfaces the `about`
  string in `about:support` under "Distribution ID", which is useful for support
  and for telling an OpenBook build apart from vanilla Firefox.
- `policies.json` — the enterprise policy file. It lives in
  `config/policies/policies.json` in this repo and is **copied into
  `distribution/` at packaging time** (kept in a separate directory here so the
  policy layer is reviewed on its own; see `config/policies/README.md`).

## How it ships

The packaging step (see `build/scripts/package.sh` and the platform packaging
hooks added by `patches/features/`) lays the directory down per platform:

- Linux: `<install>/distribution/{distribution.ini,policies.json}`
- Windows: `<install>\distribution\{distribution.ini,policies.json}`
- macOS: `<App>.app/Contents/Resources/distribution/{distribution.ini,policies.json}`

Like the AutoConfig files, the contents of `distribution/` are trusted inputs
and must be installed **root-owned and not user-writable** (see the permissions
invariant in `docs/OpenBook-Browser-Build-Plan.md` §11).

## What is deliberately NOT here

Many Firefox redistributions use `distribution.ini` `[Preferences]` /
`[LocalizablePreferences]` blocks and partner attribution parameters to set
partner search codes, homepages, and attribution pings. **OpenBook sets none of
these.** There are no partner params, no attribution codes, and no phone-home
preferences. Preference hardening is owned entirely by the locked AutoConfig
(`config/autoconfig/openbook.cfg`) and `policies.json`. This keeps the
zero-telemetry invariant easy to audit: a partner attribution code would be an
unsolicited identifier, which is forbidden.
