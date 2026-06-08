<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook patch series

Ordered patches applied on top of the pinned, verified upstream **Firefox
145.0.2** source tarball, in the LibreWolf/Mullvad model. They are the part of
OpenBook that cannot be expressed as preferences (`config/autoconfig/`),
policies (`config/policies/`) or bundled extensions.

## How they are applied

`build/scripts/apply-patches.sh` collects every file matching `*.patch` /
`*.diff` under `patches/` and applies them **sorted by path**:

```
patches/branding/0001-use-openbook-branding.patch
patches/branding/0002-neutral-default-bookmarks-and-start.patch
patches/features/0001-allow-bundled-system-extensions.patch
patches/features/0002-register-native-messaging-hosts.patch
patches/privacy/0001-remove-default-telemetry-endpoints.patch
patches/privacy/0002-disable-pocket-component.patch
```

So ordering is by directory then numeric prefix. When the extracted source is a
git worktree the runner uses `git am --3way`; otherwise it falls back to
`patch -p1`. Each patch is therefore written in **`git format-patch` style**
(with `From`/`Date`/`Subject` headers and a commit-message body) so the series is
auditable, reviewable, and rebaseable.

## Important: these are written against upstream, not validated in *this* repo

The upstream Firefox tree is **not** checked out here (it is ~20M+ lines; see
`docs/OpenBook-Browser-Build-Plan.md` §0). These patches target **real, known
upstream paths** for Firefox 145.0.2 (`browser/`, `toolkit/`, `modules/`,
`services/`, `python/`). They are structurally valid unified diffs, but their
exact line context will be **rebased in CI on a real checkout** of the pinned
tarball — that is where `apply-patches.sh` runs for real and where conflicts are
caught (per §9, CI tests patch application on every upstream release).

Treat the line numbers / surrounding context as illustrative-but-intended: the
*intent* and *target files* are correct; the *exact hunk offsets* are reconciled
against the actual 145.0.2 source on the build host.

## The series

| Patch | Purpose |
|---|---|
| `branding/0001-use-openbook-branding.patch` | Select the OpenBook branding directory in the build; stop shipping Firefox-trademarked default branding. |
| `branding/0002-neutral-default-bookmarks-and-start.patch` | Neutralize Firefox-branded default bookmarks and start/onboarding content. |
| `privacy/0001-remove-default-telemetry-endpoints.patch` | Belt-and-suspenders: neutralize compiled-in telemetry/Normandy endpoint defaults (complements the locked AutoConfig). |
| `privacy/0002-disable-pocket-component.patch` | Stop building/registering the Pocket component. |
| `features/0001-allow-bundled-system-extensions.patch` | Recognize/allowlist the three first-party OpenBook extensions in the build. |
| `features/0002-register-native-messaging-hosts.patch` | Packaging hooks so the native-messaging host manifests install root-owned (cross-refs §11). |

## Relationship to the settings layer

The privacy patches **duplicate** protections already locked in
`config/autoconfig/openbook.cfg` and `config/policies/policies.json`. That is
intentional defense in depth: removing the compiled-in endpoint defaults means
that even a code path that ignored the pref has nothing to phone home to. The
patch layer is the floor; the settings layer is enforced on top.

## TODO (real checkout / build host)

- Rebase every hunk against the verified Firefox 145.0.2 source; CI fails hard on
  conflict (§9). Update context lines as upstream moves.
- Confirm the exact build files that select branding for 145.0.2
  (`browser/moz.configure`, `browser/branding/moz.build`) and the Pocket
  build-config location, which drift between Firefox versions.
