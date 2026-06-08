// SPDX-License-Identifier: MPL-2.0
// OpenBook Vault — persistent background.
//
// Responsibilities:
//   * Hold a single long-lived VaultClient over the native host port.
//   * Lock the vault on idle (configurable timeout) so a walk-away exposes the
//     lock screen, not the unlocked profile.
//   * Relay status/commands from the UI pages via runtime messaging so the
//     pages do not each open their own native port.
//
// This file talks to `browser.*`; the unit-tested logic lives in protocol.ts
// and lockstate.ts (no browser calls there).

import { VaultClient, isErrorResponse } from "./protocol.js";

/** Default idle window before an automatic lock (seconds). */
const DEFAULT_IDLE_LOCK_SECONDS = 300;
const IDLE_PREF_KEY = "idleLockSeconds";

const client = new VaultClient();

/** Internal messages exchanged between UI pages and this background. */
type UiMessage =
  | { cmd: "status" }
  | { cmd: "setup"; secret: string; maxAttempts?: number }
  | { cmd: "unlock"; secret: string }
  | { cmd: "lock" }
  | { cmd: "erase" }
  | { cmd: "getIdleSeconds" }
  | { cmd: "setIdleSeconds"; seconds: number };

async function loadIdleSeconds(): Promise<number> {
  try {
    const got = await browser.storage.local.get(IDLE_PREF_KEY);
    const v = got[IDLE_PREF_KEY];
    return typeof v === "number" && v >= 30 ? v : DEFAULT_IDLE_LOCK_SECONDS;
  } catch {
    return DEFAULT_IDLE_LOCK_SECONDS;
  }
}

async function applyIdleInterval(): Promise<void> {
  const seconds = await loadIdleSeconds();
  // Firefox enforces a 15s minimum; our floor is 30s.
  browser.idle.setDetectionInterval(seconds);
}

/**
 * On idle, lock the vault. We only attempt a lock when the host reports an
 * unlocked state to avoid spurious commands.
 */
async function onIdleStateChanged(
  state: browser.idle.IdleState
): Promise<void> {
  if (state !== "idle") return;
  try {
    const st = await client.status();
    if (!isErrorResponse(st) && st.state === "unlocked") {
      await client.lock();
    }
  } catch {
    /* host unavailable; nothing to lock */
  }
}

async function handleUiMessage(msg: UiMessage): Promise<unknown> {
  switch (msg.cmd) {
    case "status":
      return client.status();
    case "setup":
      return client.setup({ secret: msg.secret, maxAttempts: msg.maxAttempts });
    case "unlock":
      return client.unlock(msg.secret);
    case "lock":
      return client.lock();
    case "erase":
      return client.erase();
    case "getIdleSeconds":
      return { ok: true, seconds: await loadIdleSeconds() };
    case "setIdleSeconds": {
      const seconds = Math.max(30, Math.floor(msg.seconds));
      await browser.storage.local.set({ [IDLE_PREF_KEY]: seconds });
      await applyIdleInterval();
      return { ok: true, seconds };
    }
    default:
      return { ok: false, error: "invalid-request" };
  }
}

browser.runtime.onMessage.addListener((message: unknown) => {
  // Return a promise so Firefox treats this as an async responder.
  return handleUiMessage(message as UiMessage).catch((e: unknown) => ({
    ok: false,
    error: "internal",
    message: e instanceof Error ? e.message : String(e)
  }));
});

browser.idle.onStateChanged.addListener(onIdleStateChanged);

// If the host process drops, surface nothing to the network (no telemetry);
// the next UI status call will reconnect lazily via VaultClient.connect().
client.onDisconnected(() => {
  /* intentionally silent: no logging of vault lifecycle off-device */
});

void applyIdleInterval();
