// SPDX-License-Identifier: MPL-2.0
// OpenBook Vault — setup wizard controller.
//
// Enforces (client-side, mirroring the host) the software-mode weak-secret rule
// and the explicit no-recovery consent before enabling the vault. The host
// remains authoritative; this is early, friendlier validation.

import {
  evaluateSecretStrength,
  isErrorResponse,
  DEFAULT_MAX_ATTEMPTS,
  type Hardware,
  type StatusResponse,
  type SetupOkResponse,
  type ErrorResponse
} from "./protocol.js";
import { hardwareLabel, showsSoftwareFallbackWarning } from "./lockstate.js";
import { getStatus, sendCmd, setText, show, byId } from "./ui.js";

let hardware: Hardware = "software";

function refreshValidation(): void {
  const secret = byId<HTMLInputElement>("secret")?.value ?? "";
  const confirm = byId<HTMLInputElement>("confirm")?.value ?? "";
  const ack = byId<HTMLInputElement>("ack")?.checked ?? false;
  const submit = byId<HTMLButtonElement>("submit");

  let problem = "";
  if (secret.length === 0) {
    problem = "Enter a passphrase.";
  } else {
    const strength = evaluateSecretStrength(secret, hardware);
    if (!strength.ok) {
      problem = strength.message ?? "Passphrase too weak.";
    } else if (confirm !== secret) {
      problem = "Passphrases do not match.";
    } else if (!ack) {
      problem = "You must acknowledge that this data is unrecoverable.";
    }
  }
  setText("validation", problem);
  if (submit) submit.disabled = problem.length > 0;
}

async function onSubmit(ev: Event): Promise<void> {
  ev.preventDefault();
  const secret = byId<HTMLInputElement>("secret")?.value ?? "";
  setText("status-line", "Setting up…");
  show("status-line", true);
  try {
    const res = await sendCmd<SetupOkResponse | ErrorResponse>({
      cmd: "setup",
      secret,
      maxAttempts: DEFAULT_MAX_ATTEMPTS
    });
    if (isErrorResponse(res)) {
      // Surface the host's authoritative rejection (e.g. weak-secret).
      setText("status-line", `Setup failed: ${res.error}`);
      return;
    }
    setText("status-line", "Vault enabled and locked.");
    show("done", true);
    show("form", false);
  } catch (e) {
    setText("status-line", "Could not reach the vault host.");
    // No re-throw / no logging off device.
    void e;
  }
}

async function init(): Promise<void> {
  const st: StatusResponse | ErrorResponse = await getStatus();
  if (!isErrorResponse(st)) {
    hardware = st.hardware;
    if (st.state !== "uninitialized") {
      // Already set up (or erased): do not offer setup again.
      show("form", false);
      setText("already", `Vault is already ${st.state}. Setup is unavailable.`);
      show("already", true);
    }
  }

  setText("hardware-label", hardwareLabel(hardware));
  show("software-warning", showsSoftwareFallbackWarning(hardware));
  // Reflect the minimum-strength hint only when it applies.
  show("software-strength-hint", hardware === "software");

  for (const fieldId of ["secret", "confirm"]) {
    byId<HTMLInputElement>(fieldId)?.addEventListener("input", refreshValidation);
  }
  byId<HTMLInputElement>("ack")?.addEventListener("change", refreshValidation);
  byId<HTMLFormElement>("form")?.addEventListener("submit", (e) => {
    void onSubmit(e);
  });
  refreshValidation();
}

document.addEventListener("DOMContentLoaded", () => {
  void init();
});
