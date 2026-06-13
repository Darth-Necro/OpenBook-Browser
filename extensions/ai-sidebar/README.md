<!-- SPDX-License-Identifier: MPL-2.0 -->
# OpenBook Assistant (`ai-sidebar`)

Phase 4 opt-in AI assistant. A bundled, first-party WebExtension that is
**off by default** and ships **no provider, no API key, and no remote SDK**.

- Manifest: V2, id `ai-sidebar@openbook.browser`, Firefox 145+.
- Default permissions: **`storage` only.** Host access is `optional_permissions`,
  requested **only** when the user opts in and picks a provider.
- `sidebar_action` with `sidebar.html`; settings via `options_ui`.

## Off-by-default proof (§7, security invariant 5)

The default settings are the proof, encoded in `src/registry.ts`:

```ts
DEFAULT_SETTINGS = { enabled: false, provider: null, acknowledgedEgress: false }
```

`getActiveProvider(settings)` returns **`null`** unless `enabled === true` AND a
provider is configured (and, for BYOK, the egress acknowledgement is set and the
config is complete). With the defaults there is simply no provider object to
call — `networkAllowed(DEFAULT_SETTINGS) === false`. No provider, no network
call, no telemetry until the user explicitly opts in. This is unit-tested.

Because host permissions are optional and only requested at opt-in time
(`settings.ts` → `browser.permissions.request`), the extension cannot reach any
origin out of the box even if code tried to.

## Providers (Option 3, defaulting to Option 1 local)

A minimal `Provider` interface (`src/providers/provider.ts`) so no remote SDK is
needed — implementations use plain `fetch`:

- **`LocalOllamaProvider`** (`src/providers/ollama.ts`) — POSTs to a local Ollama
  daemon (default `http://localhost:11434`). `sendsDataOffDevice === false`:
  nothing leaves the machine. No API key.
- **`BringYourOwnKeyProvider`** (`src/providers/byok.ts`) — generic
  OpenAI-compatible `/chat/completions`. The **user** supplies the base URL and
  API key; OpenBook ships neither. `sendsDataOffDevice === true`, and the
  settings UI surfaces the egress implication (your data goes to that endpoint
  under its operator's terms) and requires an explicit acknowledgement before
  the provider is usable.

`src/registry.ts` constructs **no active provider** until configured + enabled.
Zero providers are enabled at ship time.

## Prompt-injection stance (§7)

**Page content is untrusted input; prompt injection is a live, unsolved attack
class.** `src/promptguard.ts` is a **pure** function that:

- wraps any page/selection text in a clearly delimited
  `<<<OPENBOOK_UNTRUSTED_PAGE_CONTENT>>> … <<<END…>>>` block,
- prefixes an OpenBook-controlled system instruction telling the model to treat
  the block strictly as DATA, never as instructions, and to refuse embedded
  commands/role-changes,
- never silently strips or rewrites the page text (the user sees exactly what is
  sent) but **defuses forged copies of the delimiter** so a malicious page
  cannot fake an early close of the untrusted block.

The system instruction is never sourced from the page.

## Read-only default + per-action confirmation (§7)

The assistant is read-only: it analyzes text and renders replies as **text only**
(`textContent`, never `innerHTML` / never executed). Model output is untrusted
and is never auto-executed. `src/actions.ts` gates every action behind
`requiresConfirmation: true` (the type does not permit `false`) and an explicit
per-action `confirm` callback — `performAction` awaits a positive confirmation
for THAT specific action before `run` is ever invoked. Unit tests prove nothing
runs without confirmation.

## No telemetry, no bundled secrets

Zero telemetry. No bundled API key, no bundled provider, no remote SDK. The only
network calls possible are provider calls that happen **after** explicit opt-in
(and for Ollama, only to localhost).

## Files

- `src/providers/provider.ts` — `Provider` interface + config types.
- `src/providers/ollama.ts` — local provider (NDJSON stream parse).
- `src/providers/byok.ts` — BYOK OpenAI-compatible provider (SSE parse).
- `src/registry.ts` — `getActiveProvider` / `networkAllowed`, `DEFAULT_SETTINGS`.
- `src/promptguard.ts` — pure untrusted-content wrapping.
- `src/actions.ts` — per-action confirmation gate.
- `src/storage.ts` — settings persistence (off-by-default baseline merge). NOTE: a BYOK
  `apiKey` is stored in plain `storage.local` (the browser profile). It is user-supplied,
  never bundled; routing it through the vault host is a tracked hardening follow-up.
- `sidebar.html` / `src/sidebar.ts` — the sidebar UI (disabled-state when off).
- `settings.html` / `src/settings.ts` — opt-in toggle, provider pick, BYOK egress
  warning + acknowledgement, optional-permission request.
- `sidebar.css` — shared styling.
- `src/__tests__/` — jest unit tests.

## Build / test

```sh
npm install
npm run build   # tsc -p tsconfig.json (strict, zero errors)
npm test        # jest
npm run lint    # web-ext lint (best-effort)
```

## TODO (needs a real Firefox build)

- Capturing live page/selection context currently uses a user-pasted textarea to
  stay within the minimal default permission set (`storage` only). Reading the
  active tab's content would require `activeTab`/`scripting` (or a content
  script) requested at opt-in — a deliberate follow-up so the off-by-default
  permission surface stays empty until the user enables the feature.
- web-ext / Marionette / Playwright-Firefox integration: verify the sidebar
  loads, the opt-in flow requests optional host permissions, and the first-run
  egress test (`tests/privacy-regression/`) sees zero outbound connections with
  the assistant off.
