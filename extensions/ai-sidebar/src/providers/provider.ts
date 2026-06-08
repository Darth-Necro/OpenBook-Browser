// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — provider abstraction (build plan §7, Option 3).
//
// Ships ZERO providers enabled. A provider is only constructed after the user
// opts in and picks one. The interface is intentionally minimal so remote SDKs
// are never required — implementations use plain fetch.

/** A single chat turn. `system` is OpenBook-controlled; never user page text. */
export interface ChatMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

export interface CompletionRequest {
  /** Ordered messages; the prompt-guarded context is embedded as a user turn. */
  messages: ChatMessage[];
  /** Sampling temperature (provider may ignore). */
  temperature?: number;
  /** Abort signal so the UI can cancel in-flight requests. */
  signal?: AbortSignal;
}

/**
 * Provider contract. `complete` returns either a streamed token iterable or a
 * single resolved string. Model output is UNTRUSTED and never auto-executed.
 */
export interface Provider {
  /** Stable id, e.g. "ollama" | "byok". */
  readonly id: string;
  /** Human label for the settings UI. */
  readonly label: string;
  /**
   * True if using this provider sends data off the local machine. The UI MUST
   * surface this (egress implication) before enabling such a provider.
   */
  readonly sendsDataOffDevice: boolean;
  complete(req: CompletionRequest): AsyncIterable<string> | Promise<string>;
}

/** Provider configuration persisted in storage (never contains defaults). */
export interface ProviderConfig {
  id: string;
  /** Model name (provider-specific). */
  model: string;
  /** Base URL (BYOK / non-default Ollama). */
  baseUrl?: string;
  /** API key (BYOK only). Stored locally; never bundled. */
  apiKey?: string;
}
