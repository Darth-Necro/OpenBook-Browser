// SPDX-License-Identifier: MPL-2.0
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// OpenBook branding preferences.
//
// Mirrors browser/branding/<channel>/pref/firefox-branding.js. These are
// branding-scoped default prefs compiled into the build. They are NOT a
// substitute for the AutoConfig hardening in config/autoconfig/openbook.cfg;
// they only cover identity/start-surface defaults that conventionally live with
// branding. Anything security-critical (telemetry, etc.) is locked in the .cfg.
//
// HARD RULE: no telemetry, no phone-home, no Mozilla-hosted URLs. Every URL here
// is neutral (about: pages or empty). See docs/OpenBook-Browser-Build-Plan.md §4.

// Start page / home: neutral built-in home, never a remote Mozilla snippet feed.
pref("browser.startup.homepage", "about:home");
pref("browser.startup.page", 1);

// First-run and "what's new" / post-update pages: no marketing tour, no remote
// page. Empty string => no navigation. Reinforced by policies.json and the .cfg.
pref("startup.homepage_welcome_url", "");
pref("startup.homepage_welcome_url.additional", "");
pref("startup.homepage_override_url", "");

// "Learn more" / support routing. Left blank so no click silently contacts a
// Mozilla support property. Distributions may point this at OpenBook docs later.
pref("app.support.baseURL", "");
pref("app.releaseNotesURL", "");
pref("app.releaseNotesURL.aboutDialog", "");
pref("app.releaseNotesURL.prompt", "");

// Vendor/account URLs blanked: OpenBook ships no first-party account service in
// Phase 1 and must not surface Mozilla account endpoints.
pref("app.vendorURL", "");
pref("app.privacyURL", "");

// Profile-down / uninstall survey URLs: none. No exit telemetry.
pref("browser.uninstall.surveyURL", "");
