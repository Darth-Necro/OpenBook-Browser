// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — local Ollama provider (build plan §7, Option 1).
//
// Talks to a local Ollama daemon (default http://localhost:11434). NOTHING
// leaves the machine: sendsDataOffDevice is false. No API key. The host
// permission `http://localhost/*` is granted only when the user opts in.

import {
  type Provider,
  type CompletionRequest,
  type ChatMessage
} from "./provider.js";

export const DEFAULT_OLLAMA_BASE_URL = "http://localhost:11434";
export const DEFAULT_OLLAMA_MODEL = "llama3.1";

/** Shape of the Ollama /api/chat streaming JSON lines (subset). */
interface OllamaChatChunk {
  message?: { role: string; content: string };
  done?: boolean;
}

export class LocalOllamaProvider implements Provider {
  readonly id = "ollama";
  readonly label = "Local model (Ollama)";
  /** Local-only: never sends data off the device. */
  readonly sendsDataOffDevice = false;

  constructor(
    private readonly model: string = DEFAULT_OLLAMA_MODEL,
    private readonly baseUrl: string = DEFAULT_OLLAMA_BASE_URL,
    /** Injected for tests; defaults to global fetch. */
    private readonly fetchImpl: typeof fetch = fetch
  ) {}

  async *complete(req: CompletionRequest): AsyncIterable<string> {
    const url = `${this.baseUrl.replace(/\/+$/, "")}/api/chat`;
    const body = JSON.stringify({
      model: this.model,
      messages: req.messages.map((m: ChatMessage) => ({
        role: m.role,
        content: m.content
      })),
      stream: true,
      options:
        req.temperature !== undefined ? { temperature: req.temperature } : undefined
    });
    const res = await this.fetchImpl(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body,
      signal: req.signal
    });
    if (!res.ok || !res.body) {
      throw new Error(`Ollama request failed: ${res.status}`);
    }
    const reader = res.body.getReader();
    const decoder = new TextDecoder();
    let buffer = "";
    for (;;) {
      const { value, done } = await reader.read();
      if (done) break;
      buffer += decoder.decode(value, { stream: true });
      // Ollama emits newline-delimited JSON objects.
      let nl: number;
      while ((nl = buffer.indexOf("\n")) >= 0) {
        const line = buffer.slice(0, nl).trim();
        buffer = buffer.slice(nl + 1);
        if (!line) continue;
        const chunk = JSON.parse(line) as OllamaChatChunk;
        const piece = chunk.message?.content;
        if (piece) yield piece;
        if (chunk.done) return;
      }
    }
    const rest = buffer.trim();
    if (rest) {
      const chunk = JSON.parse(rest) as OllamaChatChunk;
      if (chunk.message?.content) yield chunk.message.content;
    }
  }
}
