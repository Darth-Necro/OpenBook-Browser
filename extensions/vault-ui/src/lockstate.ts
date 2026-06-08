// SPDX-License-Identifier: MPL-2.0
// OpenBook Vault — pure helpers for lock-screen presentation logic.
//
// These functions contain NO `browser.*` calls so they can be unit-tested and
// reused by both the lock screen and the background script.

import type { Hardware, VaultState } from "./protocol.js";

/** Threshold at or below which the UI must warn about imminent erasure. */
export const ERASE_WARNING_THRESHOLD = 2;

export interface AttemptWarning {
  level: "none" | "warn" | "critical";
  message: string;
}

/**
 * Decide how strongly to warn the user given remaining attempts.
 * At 0 the next state is `erased`; at <= threshold we warn that further
 * failures cryptographically erase the profile (irreversible).
 */
export function attemptWarning(attemptsRemaining: number): AttemptWarning {
  if (attemptsRemaining <= 0) {
    return {
      level: "critical",
      message:
        "No attempts remain. The profile data has been cryptographically erased and is unrecoverable."
    };
  }
  if (attemptsRemaining === 1) {
    return {
      level: "critical",
      message:
        "1 attempt remaining. The NEXT failed attempt will cryptographically erase this profile. This cannot be undone."
    };
  }
  if (attemptsRemaining <= ERASE_WARNING_THRESHOLD) {
    return {
      level: "warn",
      message: `${attemptsRemaining} attempts remaining before the profile is cryptographically erased.`
    };
  }
  return {
    level: "none",
    message: `${attemptsRemaining} attempts remaining.`
  };
}

/** Format a millisecond backoff as a short human countdown string. */
export function formatDelay(ms: number): string {
  if (ms <= 0) return "0s";
  const totalSeconds = Math.ceil(ms / 1000);
  if (totalSeconds < 60) return `${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return seconds === 0 ? `${minutes}m` : `${minutes}m ${seconds}s`;
}

/** Human label for the hardware backing, used in status surfaces. */
export function hardwareLabel(hardware: Hardware): string {
  switch (hardware) {
    case "tpm2":
      return "TPM 2.0 (hardware-enforced)";
    case "secure-enclave":
      return "Apple Secure Enclave (hardware-enforced)";
    case "software":
      return "Software fallback (weaker guarantee — no TPM/Secure Enclave)";
    default:
      return hardware;
  }
}

/** Whether the software-fallback weaker-guarantee banner should be shown. */
export function showsSoftwareFallbackWarning(hardware: Hardware): boolean {
  return hardware === "software";
}

/** Human label for a vault lifecycle state. */
export function stateLabel(state: VaultState): string {
  switch (state) {
    case "uninitialized":
      return "Not set up";
    case "locked":
      return "Locked";
    case "unlocked":
      return "Unlocked";
    case "erased":
      return "Erased (unrecoverable)";
    default:
      return state;
  }
}
