# SPDX-License-Identifier: MPL-2.0
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at https://mozilla.org/MPL/2.0/.

# OpenBook branding configure fragment.
#
# This file mirrors the role of browser/branding/<channel>/configure.sh in the
# upstream Firefox tree. When the branding patch (patches/branding/0001-*.patch)
# drops this directory in as browser/branding/openbook/, the build system sources
# this file to learn the product identity. It MUST NOT carry any Firefox or
# Mozilla trademark: per docs/OpenBook-Browser-Build-Plan.md §13 the rebrand is
# mandatory, not cosmetic.

# Internal application name: binary name, profile dir component, lowercase, no
# spaces. Distinct from the display name below.
MOZ_APP_NAME=openbook

# Human-facing product name shown in the title bar, About dialog and OS menus.
MOZ_APP_DISPLAYNAME=OpenBook

# macOS .app bundle name derived from the display name.
MOZ_MACBUNDLE_NAME=OpenBook.app

# Stable application id (XULAppData). OpenBook's own GUID — never Firefox's.
MOZ_APP_ID={c2e00de3-2c7b-4bfe-8a57-4c38e1b3a1a0}

# Vendor string baked into the application. Used for profile/registry paths,
# update URLs and the "by <vendor>" strings. OpenBook is its own vendor; it is
# explicitly NOT "Mozilla".
MOZ_APP_VENDOR=OpenBook

# Wayland/X11 app id and the -P / remoting name used by `firefox --new-instance`
# style remote control. Lowercase, no spaces. Drives the .desktop StartupWMClass
# and the DBus name, so it must be stable across releases.
MOZ_APP_REMOTINGNAME=openbook

# macOS bundle identifier base. The full CFBundleIdentifier becomes
# org.openbook.openbook for the browser app.
MOZ_MACBUNDLE_ID=org.openbook.openbook

# Distribution/Telemetry channel display label. Kept neutral; OpenBook does not
# operate Mozilla's release channels.
MOZ_DISTRIBUTION_ID=org.openbook

# Branding directory, relative to the source root, so other configure logic that
# references the branding dir resolves to ours.
MOZ_BRANDING_DIRECTORY=browser/branding/openbook
MOZ_OFFICIAL_BRANDING_DIRECTORY=browser/branding/openbook

# Do not let any upstream "official branding" trademark switch flip us back to
# Firefox art. OpenBook art is the only art we ship.
export MOZ_APP_NAME
export MOZ_MACBUNDLE_NAME
export MOZ_APP_ID
export MOZ_APP_DISPLAYNAME
export MOZ_APP_VENDOR
export MOZ_APP_REMOTINGNAME
export MOZ_MACBUNDLE_ID
export MOZ_DISTRIBUTION_ID
export MOZ_BRANDING_DIRECTORY
export MOZ_OFFICIAL_BRANDING_DIRECTORY
