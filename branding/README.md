<!-- SPDX-License-Identifier: MPL-2.0 -->

# OpenBook branding layer

This directory is OpenBook's **brand identity**: names, strings, preferences and
artwork that replace Firefox's. It mirrors the layout of Firefox's
`browser/branding/<channel>/` directory so the branding patch can drop it in
verbatim.

## Why this is mandatory, not cosmetic

Firefox **code** is MPL-2.0 (we keep the license headers and disclose
modifications). The Firefox **name and logo are Mozilla trademarks.** A fork
**must rebrand** — you cannot ship a browser called Firefox, and you cannot ship
Mozilla's logos. LibreWolf, Mullvad Browser and Tor Browser all rebrand for this
reason. See `docs/OpenBook-Browser-Build-Plan.md` §13.

Consequences enforced here:

- **No Firefox/Mozilla trademark or artwork** appears in this directory. Every
  SVG is original OpenBook work (an open-book glyph), authored for this project.
- The trademark/legal line in `locales/en-US/brand.ftl` (`trademarkInfo`) states
  plainly that OpenBook is independent and not endorsed by Mozilla.
- The `configure.sh` identity strings (`MOZ_APP_DISPLAYNAME`, `MOZ_APP_VENDOR`,
  `MOZ_APP_REMOTINGNAME`, bundle id) are all `OpenBook` / `openbook` /
  `org.openbook.*`, never Mozilla/Firefox.

## Layout

```
branding/openbook/
├── configure.sh                     # MOZ_APP_DISPLAYNAME=OpenBook, vendor, remoting name, bundle id, branding dir
├── locales/en-US/
│   ├── brand.ftl                    # Fluent brand terms ({ -brand-short-name } etc.) used across the front-end
│   └── brand.properties             # legacy .properties brand strings still read in parts of the tree
├── pref/
│   └── firefox-branding.js          # branding-scoped default prefs (start page, neutral/blank support URLs) — NO phone-home
└── content/
    ├── icon.svg                     # MASTER app icon (open-book glyph); source for all raster app icons
    ├── about-logo.svg               # wordmark + glyph for about: surfaces / About dialog (about:logo)
    ├── default-favicon.svg          # fallback favicon, simplified for 16px legibility
    └── branding.json                # manifest: maps each SVG to the raster outputs + sizes generated at build time
```

> The file name `pref/firefox-branding.js` is the **upstream-expected path** that
> Firefox's branding build logic loads; the *contents* are 100% OpenBook and
> contain no telemetry or Mozilla URLs. Keeping the path lets the branding patch
> map 1:1 without touching upstream build files.

## How it maps into the Firefox source tree

The branding patch `patches/branding/0001-branding-add-openbook-brand-directory.patch`:

1. Adds this directory into the source checkout as
   `browser/branding/openbook/`.
2. Points the build at it (`MOZ_BRANDING_DIRECTORY=browser/branding/openbook`,
   set via `configure.sh` / mozconfig) so the build uses OpenBook art and strings
   instead of the Firefox "official"/"nightly"/"unofficial" branding dirs.
3. Ensures the Firefox-trademarked default branding is **not** selected for
   release builds.

`patches/branding/0002-neutral-default-bookmarks-and-start.patch` neutralizes
the Firefox-branded default bookmarks and start content that lives outside the
branding dir.

## How raster icons are generated

The repository stays **text-only and reviewable**: we commit SVGs, not binary
`.ico`/`.icns`/`.png`. The required raster names and pixel sizes are declared in
`content/branding.json`. At build/packaging time a deterministic SVG→raster
converter (e.g. `rsvg-convert` or `resvg`, pinned in the build image) renders
them from the SVGs, for example:

```sh
# Per-size PNGs from the master icon (sizes per content/branding.json):
for s in 16 32 48 64 128 256; do
  rsvg-convert -w "$s" -h "$s" content/icon.svg -o "default${s}.png"
done

# Windows .ico (multi-resolution) and macOS .icns are then packed from those
# PNGs with icotool / iconutil (or png2icns) during packaging.
```

> The generated Windows/macOS icon files retain the upstream **file names**
> (e.g. `firefox.ico`, `firefox.icns`) only because packaging code paths expect
> those paths. The **image content is entirely OpenBook** and carries no Mozilla
> mark. This is documented in `content/branding.json`.

## TODO (requires a real Firefox checkout / build host)

- Verify exact branding asset filenames/sizes Firefox **145.0.2** expects
  (`browser/branding/<channel>/`) and reconcile `content/branding.json` against
  that list during the first real build; rebase the branding patch on it.
- Confirm current Mozilla trademark/redistribution terms before any public
  release (§13).
