// SPDX-License-Identifier: MPL-2.0
// OpenBook Vault — lock-screen controller.
//
// Shows attemptsRemaining, visualizes the host's escalating delayMs backoff
// (disabling the input during the countdown), warns as attempts approach 0 that
// the next failure cryptographically erases the profile, and handles the
// terminal `erased` state.

import {
  isErrorResponse,
  type StatusResponse,
  type UnlockOkResponse,
  type ErrorResponse
} from "./protocol.js";
import {
  attemptWarning,
  formatDelay,
  stateLabel,
  hardwareLabel,
  showsSoftwareFallbackWarning
} from "./lockstate.js";
import { getStatus, sendCmd, setText, show, byId } from "./ui.js";

let countdownTimer: ReturnType<typeof setInterval> | null = null;

function setInputsDisabled(disabled: boolean): void {
  const secret = byId<HTMLInputElement>("secret");
  const submit = byId<HTMLButtonElement>("submit");
  if (secret) secret.disabled = disabled;
  if (submit) submit.disabled = disabled;
}

function renderAttempts(attemptsRemaining: number): void {
  const w = attemptWarning(attemptsRemaining);
  setText("attempts", w.message);
  const box = byId("attempts");
  if (box) {
    box.className = `attempts attempts-${w.level}`;
  }
}

function enterErased(): void {
  if (countdownTimer) {
    clearInterval(countdownTimer);
    countdownTimer = null;
  }
  show("form", false);
  show("erased", true);
  setText(
    "erased",
    "This profile has been cryptographically erased after too many failed attempts. The data is permanently unrecoverable."
  );
}

/** Run the escalating-delay countdown; resolves when the input re-enables. */
function startCountdown(delayMs: number): void {
  if (countdownTimer) clearInterval(countdownTimer);
  let remaining = delayMs;
  setInputsDisabled(true);
  show("countdown", true);
  const tick = (): void => {
    if (remaining <= 0) {
      if (countdownTimer) clearInterval(countdownTimer);
      countdownTimer = null;
      show("countdown", false);
      setInputsDisabled(false);
      byId<HTMLInputElement>("secret")?.focus();
      return;
    }
    setText("countdown", `Locked out — try again in ${formatDelay(remaining)}.`);
    remaining -= 1000;
  };
  tick();
  countdownTimer = setInterval(tick, 1000);
}

async function onSubmit(ev: Event): Promise<void> {
  ev.preventDefault();
  const input = byId<HTMLInputElement>("secret");
  const secret = input?.value ?? "";
  if (!secret) return;
  setText("message", "Unlocking…");
  try {
    const res = await sendCmd<UnlockOkResponse | ErrorResponse>({
      cmd: "unlock",
      secret
    });
    if (isErrorResponse(res)) {
      if (res.error === "erased" || res.state === "erased") {
        enterErased();
        return;
      }
      if (res.error === "bad-secret") {
        if (input) input.value = "";
        if (typeof res.attemptsRemaining === "number") {
          renderAttempts(res.attemptsRemaining);
          if (res.attemptsRemaining <= 0) {
            enterErased();
            return;
          }
        }
        setText("message", "Incorrect passphrase.");
        if (typeof res.delayMs === "number" && res.delayMs > 0) {
          startCountdown(res.delayMs);
        }
        return;
      }
      setText("message", `Error: ${res.error}`);
      return;
    }
    // Unlocked.
    setText("message", "Unlocked.");
    show("form", false);
    show("unlocked", true);
  } catch {
    setText("message", "Could not reach the vault host.");
  }
}

async function init(): Promise<void> {
  const st: StatusResponse | ErrorResponse = await getStatus();
  if (isErrorResponse(st)) {
    setText("message", "Vault host unavailable.");
    setInputsDisabled(true);
    return;
  }
  setText("hardware-label", hardwareLabel(st.hardware));
  show("software-warning", showsSoftwareFallbackWarning(st.hardware));
  setText("state-label", stateLabel(st.state));

  if (st.state === "erased") {
    enterErased();
    return;
  }
  if (st.state === "unlocked") {
    show("form", false);
    show("unlocked", true);
    return;
  }
  if (st.state === "uninitialized") {
    show("form", false);
    setText("message", "Vault is not set up yet.");
    return;
  }
  renderAttempts(st.attemptsRemaining);
  byId<HTMLFormElement>("form")?.addEventListener("submit", (e) => {
    void onSubmit(e);
  });
  byId<HTMLInputElement>("secret")?.focus();
}

document.addEventListener("DOMContentLoaded", () => {
  void init();
});
