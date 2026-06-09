// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — provider registry (build plan §7).
//
// OFF BY DEFAULT. `getActiveProvider` returns null unless the assistant is
// explicitly enabled AND a provider is configured. This is the code-level proof
// that no network call can occur out of the box: with the default settings
// there is simply no provider to call.

import { type Provider, type ProviderConfig } from "./providers/provider.js";
import { LocalOllamaProvider, DEFAULT_OLLAMA_BASE_URL } from "./providers/ollama.js";
import { BringYourOwnKeyProvider } from "./providers/byok.js";

/** Persisted assistant settings. Defaults below are the off-by-default state. */
export interface AssistantSettings {
  /** Master switch. MUST default to false. */
  enabled: boolean;
  /** Active provider config, or null when none chosen. MUST default to null. */
  provider: ProviderConfig | null;
  /**
   * For BYOK, the user must explicitly acknowledge the egress implication
   * before the provider is usable. MUST default to false.
   */
  acknowledgedEgress: boolean;
}

/** The canonical off-by-default settings. No provider, disabled, no egress ack. */
export const DEFAULT_SETTINGS: AssistantSettings = {
  enabled: false,
  provider: null,
  acknowledgedEgress: false
};

/** Provider ids we know how to build. */
export const KNOWN_PROVIDER_IDS = ["ollama", "byok"] as const;
export type KnownProviderId = (typeof KNOWN_PROVIDER_IDS)[number];

/**
 * Build the active provider from settings, or return null. PURE (fetch is
 * injected into the constructed provider, not called here).
 *
 * Returns null when:
 *   - the assistant is disabled, OR
 *   - no provider is configured, OR
 *   - the provider id is unknown, OR
 *   - a BYOK provider lacks the egress acknowledgement, OR
 *   - required fields are missing.
 *
 * @param fetchImpl optional fetch to inject into the provider (tests).
 */
export function getActiveProvider(
  settings: AssistantSettings,
  fetchImpl?: typeof fetch
): Provider | null {
  if (!settings.enabled) return null;
  const cfg = settings.provider;
  if (!cfg) return null;

  const f = fetchImpl ?? (typeof fetch !== "undefined" ? fetch : undefined);

  switch (cfg.id) {
    case "ollama":
      return new LocalOllamaProvider(
        cfg.model || undefined,
        cfg.baseUrl || DEFAULT_OLLAMA_BASE_URL,
        f as typeof fetch
      );
    case "byok": {
      // Remote provider: require explicit egress acknowledgement + full config.
      if (!settings.acknowledgedEgress) return null;
      if (!cfg.baseUrl || !cfg.apiKey || !cfg.model) return null;
      return new BringYourOwnKeyProvider(
        cfg.baseUrl,
        cfg.apiKey,
        cfg.model,
        f as typeof fetch
      );
    }
    default:
      return null; // unknown id: never construct anything
  }
}

/** True when the assistant may make ANY network call given these settings. */
export function networkAllowed(settings: AssistantSettings): boolean {
  return getActiveProvider(settings) !== null;
}
