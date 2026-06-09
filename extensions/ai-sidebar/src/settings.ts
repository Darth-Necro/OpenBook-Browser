// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — settings controller.
//
// Implements the opt-in: enable toggle, provider pick, and the BYOK egress
// acknowledgement. Optional HOST permissions are requested ONLY here, only when
// the user enables a provider that needs them. Until then the extension holds
// just "storage" and can make no network call.

import {
  type AssistantSettings,
  DEFAULT_SETTINGS,
  getActiveProvider
} from "./registry.js";
import { type ProviderConfig } from "./providers/provider.js";
import { DEFAULT_OLLAMA_BASE_URL, DEFAULT_OLLAMA_MODEL } from "./providers/ollama.js";
import { loadSettings, saveSettings } from "./storage.js";

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

let settings: AssistantSettings = { ...DEFAULT_SETTINGS };

/** Map the selected provider id to the optional host permissions it needs. */
function originsFor(providerId: string, baseUrl: string): string[] {
  if (providerId === "ollama") {
    // Local daemon only.
    return ["http://localhost/*", "http://127.0.0.1/*"];
  }
  if (providerId === "byok") {
    try {
      const u = new URL(baseUrl);
      return [`${u.protocol}//${u.host}/*`];
    } catch {
      return ["https://*/*"];
    }
  }
  return [];
}

function readForm(): { providerId: string; cfg: ProviderConfig; baseUrl: string } {
  const providerId = byId<HTMLSelectElement>("provider")?.value ?? "";
  const model = byId<HTMLInputElement>("model")?.value.trim() ?? "";
  const baseUrl = byId<HTMLInputElement>("base-url")?.value.trim() ?? "";
  const apiKey = byId<HTMLInputElement>("api-key")?.value ?? "";
  const cfg: ProviderConfig = {
    id: providerId,
    model:
      model || (providerId === "ollama" ? DEFAULT_OLLAMA_MODEL : ""),
    baseUrl:
      baseUrl || (providerId === "ollama" ? DEFAULT_OLLAMA_BASE_URL : undefined),
    apiKey: providerId === "byok" ? apiKey : undefined
  };
  return { providerId, cfg, baseUrl: cfg.baseUrl ?? "" };
}

function refreshProviderFields(): void {
  const providerId = byId<HTMLSelectElement>("provider")?.value ?? "";
  const isByok = providerId === "byok";
  show("byok-fields", isByok);
  show("egress-warning", isByok);
  // The local-only reassurance is shown for Ollama.
  show("local-note", providerId === "ollama");
}

async function onEnableChange(): Promise<void> {
  const enabled = byId<HTMLInputElement>("enabled")?.checked ?? false;
  if (!enabled) {
    // Disabling: persist off-by-default-equivalent immediately. We keep the
    // chosen provider config but flip enabled=false so no calls happen.
    settings = { ...settings, enabled: false };
    await saveSettings(settings);
    setText("status", "Assistant disabled. No provider will be contacted.");
    renderActiveLine();
    return;
  }
  // Enabling requires a fully valid provider + (for BYOK) egress ack + perms.
  await onSave();
}

async function onSave(): Promise<void> {
  const enabled = byId<HTMLInputElement>("enabled")?.checked ?? false;
  const acknowledgedEgress = byId<HTMLInputElement>("ack-egress")?.checked ?? false;
  const { providerId, cfg, baseUrl } = readForm();

  if (!providerId) {
    setText("status", "Pick a provider.");
    return;
  }
  if (providerId === "byok") {
    if (!cfg.baseUrl || !cfg.apiKey || !cfg.model) {
      setText("status", "BYOK needs a base URL, API key, and model name.");
      return;
    }
    if (enabled && !acknowledgedEgress) {
      setText(
        "status",
        "Acknowledge that your data will be sent to that endpoint before enabling."
      );
      return;
    }
  }

  const candidate: AssistantSettings = {
    enabled,
    provider: cfg,
    acknowledgedEgress
  };

  // Request optional host permissions only when actually enabling.
  if (enabled) {
    const origins = originsFor(providerId, baseUrl);
    try {
      const granted = await browser.permissions.request({ origins });
      if (!granted) {
        setText(
          "status",
          "Host permission was denied; the assistant stays off until granted."
        );
        // Do not enable without the permission.
        candidate.enabled = false;
      }
    } catch {
      candidate.enabled = false;
      setText("status", "Could not request host permission; staying off.");
    }
  }

  settings = candidate;
  await saveSettings(settings);
  renderActiveLine();
  if (settings.enabled) {
    setText("status", "Assistant enabled.");
  }
}

function renderActiveLine(): void {
  const provider = getActiveProvider(settings);
  if (!provider) {
    setText("active-line", "Status: OFF — no provider active, no network calls.");
    return;
  }
  setText(
    "active-line",
    provider.sendsDataOffDevice
      ? `Status: ON — ${provider.label} (data leaves this device).`
      : `Status: ON — ${provider.label} (local only).`
  );
}

function fillForm(): void {
  const enabled = byId<HTMLInputElement>("enabled");
  if (enabled) enabled.checked = settings.enabled;
  const ack = byId<HTMLInputElement>("ack-egress");
  if (ack) ack.checked = settings.acknowledgedEgress;
  if (settings.provider) {
    const sel = byId<HTMLSelectElement>("provider");
    if (sel) sel.value = settings.provider.id;
    const model = byId<HTMLInputElement>("model");
    if (model) model.value = settings.provider.model;
    const baseUrl = byId<HTMLInputElement>("base-url");
    if (baseUrl) baseUrl.value = settings.provider.baseUrl ?? "";
    const apiKey = byId<HTMLInputElement>("api-key");
    if (apiKey) apiKey.value = settings.provider.apiKey ?? "";
  }
}

async function init(): Promise<void> {
  settings = await loadSettings();
  fillForm();
  refreshProviderFields();
  renderActiveLine();
  byId<HTMLSelectElement>("provider")?.addEventListener("change", refreshProviderFields);
  byId<HTMLInputElement>("enabled")?.addEventListener("change", () => {
    void onEnableChange();
  });
  byId<HTMLButtonElement>("save")?.addEventListener("click", () => {
    void onSave();
  });
}

document.addEventListener("DOMContentLoaded", () => {
  void init();
});
