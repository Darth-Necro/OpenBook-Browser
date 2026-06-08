<!-- SPDX-License-Identifier: MPL-2.0 -->
# Native messaging manifest — `org.openbook.vault_host`

`org.openbook.vault_host.json` is the Firefox **native messaging host manifest**
template for the OpenBook vault host. Firefox reads and validates this manifest
but does **not** install or manage the host — its security model is that of a
native application (Build Plan §1, §11). Installers/packagers are responsible for
placing the manifest and binary correctly.

## Placeholders to substitute at packaging time

- `<INSTALL_PREFIX>` → the install root. The `path` must point at the installed
  `openbook-vault-host` binary, e.g. `/usr/lib/openbook/openbook-vault-host`.
  On Windows the manifest's `path` must be an absolute path to the `.exe`, and a
  registry key references the manifest (see below).

`allowed_extensions` is pinned to **`vault-ui@openbook.browser`** — only that
extension may connect. Keep it in sync with `extensions/vault-ui`'s gecko id.

## Per-OS manifest locations (Firefox/forks)

For a system-wide install (preferred for OpenBook, so files are root-owned):

- **Linux:** `/usr/lib/mozilla/native-messaging-hosts/org.openbook.vault_host.json`
  (per-user alternative: `~/.mozilla/native-messaging-hosts/`).
- **macOS:** `/Library/Application Support/Mozilla/NativeMessagingHosts/org.openbook.vault_host.json`
  (per-user: `~/Library/Application Support/Mozilla/NativeMessagingHosts/`).
- **Windows:** place the JSON anywhere readable and set the registry value
  `HKEY_LOCAL_MACHINE\SOFTWARE\Mozilla\NativeMessagingHosts\org.openbook.vault_host`
  (default value = absolute path to the JSON). Per-user uses `HKEY_CURRENT_USER`.

The OpenBook fork may also relocate these under its own vendor directory; if so,
mirror the same relative layout under the fork's data dir.

## PERMISSIONS INVARIANT (release blocker — Build Plan §11)

The manifest **and** the host binary must be installed **root-owned and not
user-writable** in release packages:

- A user-writable native-host binary or manifest is a local privilege-escalation
  hole: an attacker who can rewrite either can run arbitrary native code in the
  user's session the next time the extension connects.
- On Unix: `root:root`, mode `0644` for the manifest, `0755` for the binary, in a
  directory that is itself not user-writable (e.g. `0755 root:root`).
- On Windows: the binary and manifest must live under a location writable only by
  administrators (e.g. `%ProgramFiles%`), and the registry key under HKLM.

Treat any deviation (user-writable privileged file) as a release blocker. This is
the same class of finding as user-writable AutoConfig (`openbook.cfg`).
