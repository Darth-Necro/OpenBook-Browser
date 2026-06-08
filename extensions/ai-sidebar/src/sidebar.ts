// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — sidebar controller. Browser glue; pure logic imported.
//
// OFF BY DEFAULT: if getActiveProvider returns null, the sidebar shows the
// disabled state and makes NO network call. Page/selection context is treated
// as UNTRUSTED and wrapped by promptguard before being sent. Model output is
// rendered as text only and never executed.

import { getActiveProvider, type AssistantSettings } from "./registry.js";
import { guardedMessages } from "./promptguard.js";
import { loadSettings } from "./storage.js";
import { type CompletionRequest } from "./providers/provider.js";

function byId<T extends HTMLElement = HTMLElement>(id: string): T | null {
  return document.getElementById(id) as T | null;
}
function setText(id: string, text: string): void {
  const el = byId(id);
  if (el) el.textContent = text;
}
function show(id: string, visible: boolean): void {
  const el = byId(id);
  if (el) el.hidden = !visible;
}

let settings: AssistantSettings;
let inFlight: AbortController | null = null;

async function consumeToOutput(
  out: AsyncIterable<string> | Promise<string>
): Promise<void> {
  const outputEl = byId("output");
  if (!outputEl) return;
  outputEl.textContent = "";
  if (typeof (out as AsyncIterable<string>)[Symbol.asyncIterator] === "function") {
    for await (const piece of out as AsyncIterable<string>) {
      // textContent append => model output is never parsed as HTML/executed.
      outputEl.textContent += piece;
    }
  } else {
    outputEl.textContent = await (out as Promise<string>);
  }
}

async function onAsk(): Promise<void> {
  const provider = getActiveProvider(settings);
  if (!provider) {
    // Off by default: refuse to call anything.
    setText("status", "Assistant is off. Enable it in settings to use it.");
    return;
  }
  const question = byId<HTMLTextAreaElement>("question")?.value.trim() ?? "";
  const context = byId<HTMLTextAreaElement>("context")?.value ?? "";
  if (!question) {
    setText("status", "Type a question first.");
    return;
  }
  setText("status", provider.sendsDataOffDevice ? "Sending to remote provider…" : "Asking local model…");

  inFlight?.abort();
  inFlight = new AbortController();
  const req: CompletionRequest = {
    messages: guardedMessages(question, context),
    signal: inFlight.signal
  };
  try {
    await consumeToOutput(provider.complete(req));
    setText("status", "Done.");
  } catch (e) {
    setText("status", `Request failed: ${e instanceof Error ? e.message : String(e)}`);
  } finally {
    inFlight = null;
  }
}

function renderEnabledState(): void {
  const provider = getActiveProvider(settings);
  const enabled = provider !== null;
  show("composer", enabled);
  show("disabled-notice", !enabled);
  if (enabled && provider) {
    setText(
      "provider-label",
      provider.sendsDataOffDevice
        ? `${provider.label} — data leaves this device`
        : `${provider.label} — local only`
    );
  }
}

async function init(): Promise<void> {
  settings = await loadSettings();
  renderEnabledState();
  byId<HTMLButtonElement>("ask")?.addEventListener("click", () => {
    void onAsk();
  });
  byId<HTMLButtonElement>("open-settings")?.addEventListener("click", () => {
    void browser.runtime.openOptionsPage();
  });
  // Re-read settings when they change (opt-in toggled in the settings page).
  browser.storage.onChanged.addListener((_changes, area) => {
    if (area === "local") {
      void loadSettings().then((s) => {
        settings = s;
        renderEnabledState();
      });
    }
  });
}

document.addEventListener("DOMContentLoaded", () => {
  void init();
});
