<!-- SPDX-License-Identifier: MPL-2.0 -->
# OpenBook Vault (`vault-ui`)

Phase 2 cryptographic-lockout UI. A bundled, first-party WebExtension that is
the user-facing surface for the OpenBook profile vault. All cryptography,
hardware sealing (TPM 2.0 / Secure Enclave), the monotonic attempt counter, and
the irreversible key erasure happen in the Rust **native messaging host**
`org.openbook.vault_host`; this extension only presents state and relays
commands.

- Manifest: V2, id `vault-ui@openbook.browser`, Firefox 145+.
- Permissions: `nativeMessaging`, `storage` (only the idle-lock preference), and `idle`
  (drives auto-lock when the user walks away).
- No network access. No telemetry. The vault host name and the extension id are
  fixed by contract.

## Native messaging protocol

Length-prefixed JSON over stdio (the browser frames this; the extension sees
`browser.runtime` port messages). Requests are `{type,id,...}`; every response
echoes `id` and carries `ok:boolean`. Correlation is by `id`
(`src/protocol.ts` → `VaultClient`).

| Request | Fields | Success | Error |
|---|---|---|---|
| `status` | — | `{ok:true,state,hardware,maxAttempts,attemptsRemaining}` | `{ok:false,error}` |
| `setup` | `secret,maxAttempts(=6),acknowledgeNoRecovery:true` | `{ok:true,state:"locked"}` | `weak-secret`, `already-initialized`, `no-recovery-not-acknowledged`, … |
| `unlock` | `secret` | `{ok:true,state:"unlocked"}` | `{ok:false,error:"bad-secret",attemptsRemaining,delayMs}` or `{ok:false,error:"erased",state:"erased"}` |
| `lock` | — | `{ok:true,state:"locked"}` | `{ok:false,error}` |
| `erase` | `confirm:true` | `{ok:true,state:"erased"}` | `{ok:false,error}` |

- `state`: `uninitialized | locked | unlocked | erased`
- `hardware`: `tpm2 | secure-enclave | software`
- Error envelope: `{ok:false,error:string,message?:string}`. Codes:
  `invalid-request`, `not-initialized`, `already-initialized`, `bad-secret`,
  `erased`, `weak-secret`, `no-recovery-not-acknowledged`,
  `hardware-unavailable`, `internal`.

The host is authoritative; the UI validates early but never relies on
client-side checks for security.

## Hardware-fallback labeling (security invariant 4)

When `status.hardware === "software"` (no TPM 2.0 / Secure Enclave):

- The setup wizard **requires a strong passphrase** — it blocks all-digit
  secrets and any secret shorter than 12 characters, mirroring the host's
  `weak-secret` rule (`evaluateSecretStrength` in `src/protocol.ts`, the single
  source of truth shared with the unit tests).
- Every surface (setup, lock, options) shows a prominent banner stating the
  guarantee is **weaker**: without hardware the attempt limit is advisory
  against an attacker who images the disk, and protection rests on passphrase
  strength via Argon2id.

With `tpm2` / `secure-enclave` the hardware rate-limits and enforces the
counter, so the strong-passphrase requirement is relaxed.

## No-recovery consent

Setup cannot proceed until the user ticks "I understand this data is
unrecoverable", and the `setup` request always carries
`acknowledgeNoRecovery:true`. The host independently rejects setup lacking the
acknowledgement (`no-recovery-not-acknowledged`).

## Escalating delay and erasure warnings

The lock screen renders the host's `delayMs` as a live countdown that disables
the passphrase input, and uses `attemptWarning` to escalate the messaging as
`attemptsRemaining` approaches 0 — at 1 remaining it warns that the **next**
failure cryptographically erases the profile. The terminal `erased` state
replaces the form with a permanent unrecoverable-data notice.

## Permissions invariant for the native host (build plan §11)

The browser **validates but does not install or manage** the native host. The
host binary and its manifest (which lists `vault-ui@openbook.browser` in
`allowed_extensions`) must be installed **root-owned and not user-writable** in
release packages. A user-writable native host is a local privilege-escalation
hole and is a release blocker. This extension assumes that packaging guarantee.

## Destructive flows are exercised only against the disposable harness

Setup-with-erasure, unlock-to-exhaustion, and explicit erase destroy profile
data irreversibly (cryptographic erasure — invalidating keys, not deleting
files). Per security invariant 7, those flows are exercised **only** against
disposable VMs/containers with throwaway profiles in the native-host /
integration harness, never on a developer host. The jest tests here cover only
pure logic (protocol (de)serialization, id-correlation, the weak-secret rule)
and never invoke the host.

## Files

- `src/protocol.ts` — wire types + `VaultClient` (id-correlated native port).
- `src/lockstate.ts` — pure presentation helpers (attempt warnings, delay
  formatting, hardware/state labels).
- `src/background.ts` — persistent background: single native connection,
  lock-on-idle, message relay for the UI pages.
- `src/ui.ts` — thin UI↔background messaging glue.
- `src/setup.ts` / `setup.html` — setup wizard.
- `src/lock.ts` / `lock.html` — lock screen.
- `src/options.ts` / `options.html` — status, lock-now, idle timeout, erase
  (double-confirm).
- `vault.css` — shared styling.
- `src/__tests__/` — jest unit tests.

## Build / test

```sh
npm install
npm run build   # tsc -p tsconfig.json (strict, zero errors)
npm test        # jest
npm run lint    # web-ext lint -s dist (best-effort; needs a built dist)
```

## TODO (needs a real Firefox build)

- web-ext / Marionette / Playwright-Firefox integration against a built browser
  with the native host present (the destructive harness in `tests/native/`).
- The lock screen is wired as a normal extension page; the build plan's
  full-screen lock-on-startup gating is an engine/patch concern, not this
  extension alone.
