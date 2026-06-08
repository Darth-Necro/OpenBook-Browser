// SPDX-License-Identifier: MPL-2.0
// OpenBook Vault — options/status controller.
//
// Shows hardware kind + state, a lock-now button, an idle-lock timeout setting,
// and an explicit erase action gated behind a double confirmation (typed token
// + checkbox) because erasure is irreversible cryptographic destruction.

import {
  isErrorResponse,
  type StatusResponse,
  type LockOkResponse,
  type EraseOkResponse,
  type ErrorResponse
} from "./protocol.js";
import {
  hardwareLabel,
  stateLabel,
  showsSoftwareFallbackWarning
} from "./lockstate.js";
import { getStatus, sendCmd, setText, show, byId } from "./ui.js";

const ERASE_TOKEN = "ERASE";

async function refresh(): Promise<void> {
  const st: StatusResponse | ErrorResponse = await getStatus();
  if (isErrorResponse(st)) {
    setText("state-value", "Host unavailable");
    setText("hardware-value", "—");
    return;
  }
  setText("state-value", stateLabel(st.state));
  setText("hardware-value", hardwareLabel(st.hardware));
  setText(
    "attempts-value",
    `${st.attemptsRemaining} of ${st.maxAttempts} attempts remaining`
  );
  show("software-warning", showsSoftwareFallbackWarning(st.hardware));

  // Lock button only meaningful when unlocked.
  const lockBtn = byId<HTMLButtonElement>("lock-now");
  if (lockBtn) lockBtn.disabled = st.state !== "unlocked";

  // Erase only meaningful when initialized and not already erased.
  const eraseFieldset = byId<HTMLFieldSetElement>("erase-fieldset");
  if (eraseFieldset) {
    eraseFieldset.disabled = st.state === "uninitialized" || st.state === "erased";
  }
}

async function onLockNow(): Promise<void> {
  setText("action-message", "Locking…");
  const res = await sendCmd<LockOkResponse | ErrorResponse>({ cmd: "lock" });
  setText(
    "action-message",
    isErrorResponse(res) ? `Lock failed: ${res.error}` : "Vault locked."
  );
  await refresh();
}

function refreshEraseGate(): void {
  const token = byId<HTMLInputElement>("erase-token")?.value ?? "";
  const checked = byId<HTMLInputElement>("erase-confirm")?.checked ?? false;
  const btn = byId<HTMLButtonElement>("erase-now");
  if (btn) btn.disabled = !(token === ERASE_TOKEN && checked);
}

async function onErase(): Promise<void> {
  // Double-confirm already enforced by the gate; final guard here.
  const token = byId<HTMLInputElement>("erase-token")?.value ?? "";
  const checked = byId<HTMLInputElement>("erase-confirm")?.checked ?? false;
  if (token !== ERASE_TOKEN || !checked) return;
  setText("action-message", "Erasing…");
  const res = await sendCmd<EraseOkResponse | ErrorResponse>({ cmd: "erase" });
  if (isErrorResponse(res)) {
    setText("action-message", `Erase failed: ${res.error}`);
  } else {
    setText(
      "action-message",
      "Vault erased. Profile data is permanently unrecoverable."
    );
  }
  await refresh();
}

async function loadIdle(): Promise<void> {
  const res = await sendCmd<{ ok: boolean; seconds?: number }>({
    cmd: "getIdleSeconds"
  });
  const input = byId<HTMLInputElement>("idle-seconds");
  if (input && typeof res.seconds === "number") {
    input.value = String(res.seconds);
  }
}

async function onSaveIdle(): Promise<void> {
  const input = byId<HTMLInputElement>("idle-seconds");
  const seconds = Math.max(30, Math.floor(Number(input?.value ?? "300")));
  const res = await sendCmd<{ ok: boolean; seconds?: number }>({
    cmd: "setIdleSeconds",
    seconds
  });
  setText(
    "idle-message",
    res.ok ? `Auto-lock set to ${res.seconds}s of idle.` : "Could not save."
  );
}

function init(): void {
  byId<HTMLButtonElement>("lock-now")?.addEventListener("click", () => {
    void onLockNow();
  });
  byId<HTMLButtonElement>("erase-now")?.addEventListener("click", () => {
    void onErase();
  });
  byId<HTMLInputElement>("erase-token")?.addEventListener("input", refreshEraseGate);
  byId<HTMLInputElement>("erase-confirm")?.addEventListener("change", refreshEraseGate);
  byId<HTMLButtonElement>("save-idle")?.addEventListener("click", () => {
    void onSaveIdle();
  });
  setText("erase-token-hint", `Type ${ERASE_TOKEN} to enable the erase button.`);
  refreshEraseGate();
  void refresh();
  void loadIdle();
}

document.addEventListener("DOMContentLoaded", init);
