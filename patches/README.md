<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook patch series

Ordered patches applied on top of the pinned, verified upstream **Firefox
145.0.2** source tarball, in the LibreWolf/Mullvad model. They are the part of
OpenBook that cannot be expressed as preferences (`config/autoconfig/`),
policies (`config/policies/`) or bundled extensions.

## How they are applied

`build/scripts/apply-patches.sh` applies the phases in the documented order —
**branding, then privacy, then features** — sorting by path *within* each
phase (`LC_ALL=C`):

```
patches/branding/0001-branding-add-openbook-brand-directory.patch
patches/branding/0002-neutral-default-bookmarks-and-start.patch
patches/privacy/0001-remove-default-telemetry-endpoints.patch
patches/features/0002-register-native-messaging-hosts.patch
```

When the extracted source is a git worktree the runner uses `git am --3way`;
otherwise it falls back to `patch -p1 --forward --fuzz=0`. **Fuzz is zero on
purpose:** a privacy hunk applying at drifted context is a silent semantic
change, so the series demands exact line matches (offsets are allowed,
fuzzy context is not). Each patch is written in **`git format-patch` style**
(`From`/`Date`/`Subject` headers and a commit-message body) so the series is
auditable, reviewable, and rebaseable.

## Authoring rules (learned the hard way)

1. **A patch must never list its own output as context.** Lines OpenBook adds
   are `+` additions; context lines must be REAL upstream lines. (An earlier
   revision of the packaging patch listed `openbook.cfg` as pre-existing
   context — such a hunk can never apply anywhere, and worse, would have let
   `mach package` ship a build without the hardening layer.)
2. **Anchor hunks on the most stable upstream strings** (e.g. the pref lines
   being replaced), not on volatile surroundings.
3. **Prefer no patch at all.** Bundled extensions use Mozilla's supported
   `distribution/extensions/` mechanism (auto-install on first run +
   `ExtensionSettings` allowlist in policies.json) — the earlier
   `builtinExtensions.json` patch targeted a file that does not exist upstream
   and was removed. The Pocket-removal patch was dropped likewise: Mozilla
   discontinued and removed Pocket from Firefox before 145, so upstream
   already achieves its goal (the cfg/policy locks remain as belt and
   suspenders).

## Important: these are written against upstream, not validated in *this* repo

The upstream Firefox tree is **not** checked out here (it is ~20M+ lines; see
`docs/OpenBook-Browser-Build-Plan.md` §0). These patches target real upstream
paths for Firefox 145.0.2. They are structurally valid unified diffs
(`git apply --stat` clean), but their exact hunk offsets are reconciled on a
real checkout of the pinned tarball — `apply-patches.sh` runs there and fails
hard on any conflict (per §9, CI tests patch application on every upstream
release; see also `docs/RELEASE-CHECKLIST.md` §3).

## The series

| Patch | Purpose |
|---|---|
| `branding/0001-branding-add-openbook-brand-directory.patch` | Add `browser/branding/openbook/` (identical to the repo's `branding/openbook/`, which build.sh stages over it); selected via `--with-branding`. Raster icon generation is a tracked release blocker (see `branding/openbook/content/branding.json`). |
| `branding/0002-neutral-default-bookmarks-and-start.patch` | Neutralize Firefox-branded default bookmarks and start/onboarding content. |
| `privacy/0001-remove-default-telemetry-endpoints.patch` | Belt-and-suspenders: neutralize compiled-in telemetry/Normandy endpoint defaults at their real locations (complements the locked AutoConfig). |
| `features/0002-register-native-messaging-hosts.patch` | Package the hardening layer (autoconfig.js, openbook.cfg, policies.json), the three distribution XPIs, and the native-messaging host manifests/binary — all as additions (§11 root-owned at install time). |

## Relationship to the settings layer

The privacy patches **duplicate** protections already locked in
`config/autoconfig/openbook.cfg` and `config/policies/policies.json`. That is
intentional defense in depth: removing the compiled-in endpoint defaults means
that even a code path that ignored the pref has nothing to phone home to. The
patch layer is the floor; the settings layer is enforced on top.

## TODO (real checkout / build host)

- Rebase every hunk against the verified Firefox 145.0.2 source; CI fails hard
  on conflict (§9). Update offsets as upstream moves.
- Land the deterministic SVG→raster branding generation step and restore the
  raster lines in `branding/openbook/jar.mn` (release blocker, §13).
- Stage built XPIs into `dist/distribution/extensions/` on the build host
  before `mach package` (the manifest lines in `features/0002` expect them).
