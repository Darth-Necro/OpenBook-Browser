// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — Bring-Your-Own-Key provider (build plan §7, Option 2).
//
// Generic OpenAI-compatible /chat/completions client. The USER supplies the
// base URL + API key; OpenBook ships NEITHER. sendsDataOffDevice is true, and
// the settings UI MUST surface the egress implication (browsing context goes to
// that endpoint under its operator's terms) before this provider is enabled.

import {
  type Provider,
  type CompletionRequest,
  type ChatMessage
} from "./provider.js";

/** Subset of an OpenAI-compatible streaming chunk. */
interface OpenAiStreamChunk {
  choices?: Array<{ delta?: { content?: string }; finish_reason?: string | null }>;
}

export class BringYourOwnKeyProvider implements Provider {
  readonly id = "byok";
  readonly label = "Bring your own API key (OpenAI-compatible)";
  /** Remote: data goes to the user-supplied endpoint. Surfaced in the UI. */
  readonly sendsDataOffDevice = true;

  constructor(
    private readonly baseUrl: string,
    private readonly apiKey: string,
    private readonly model: string,
    /** Injected for tests; defaults to global fetch. */
    private readonly fetchImpl: typeof fetch = fetch
  ) {
    if (!baseUrl) throw new Error("BYOK provider requires a base URL");
    if (!apiKey) throw new Error("BYOK provider requires an API key");
    if (!model) throw new Error("BYOK provider requires a model name");
  }

  async *complete(req: CompletionRequest): AsyncIterable<string> {
    const url = `${this.baseUrl.replace(/\/+$/, "")}/chat/completions`;
    const body = JSON.stringify({
      model: this.model,
      messages: req.messages.map((m: ChatMessage) => ({
        role: m.role,
        content: m.content
      })),
      stream: true,
      temperature: req.temperature
    });
    const res = await this.fetchImpl(url, {
      method: "POST",
      headers: {
        "Content-Type": "application/json",
        Authorization: `Bearer ${this.apiKey}`
      },
      body,
      signal: req.signal
    });
    if (!res.ok || !res.body) {
      throw new Error(`Provider request failed: ${res.status}`);
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      // Server-Sent Events: lines prefixed with "data: ".
      let nl: number;
      while ((nl = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        if (!line || !line.startsWith("data:")) continue;
        const payload = line.slice(5).trim();
        if (payload === "[DONE]") return;
        const chunk = JSON.parse(payload) as OpenAiStreamChunk;
        const piece = chunk.choices?.[0]?.delta?.content;
        if (piece) yield piece;
      }
    }
  }
}
