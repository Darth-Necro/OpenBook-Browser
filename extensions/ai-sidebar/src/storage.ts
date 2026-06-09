// SPDX-License-Identifier: MPL-2.0
// OpenBook Assistant — settings persistence. Browser glue only.

import { type AssistantSettings, DEFAULT_SETTINGS } from "./registry.js";

const KEY = "assistantSettings";

export async function loadSettings(): Promise<AssistantSettings> {
  try {
    const got = await browser.storage.local.get(KEY);
    const saved = got[KEY] as Partial<AssistantSettings> | undefined;
    // Always start from the off-by-default baseline; merge saved over it.
    return { ...DEFAULT_SETTINGS, ...(saved ?? {}) };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}

export async function saveSettings(settings: AssistantSettings): Promise<void> {
  await browser.storage.local.set({ [KEY]: settings });
}
