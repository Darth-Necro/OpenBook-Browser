// SPDX-License-Identifier: MPL-2.0
// OpenBook Proxy — PURE fail-closed decision logic (no browser.* calls).
//
// Security invariant 2 (build plan §6 control 4): if the proxy/tunnel drops,
// BLOCK traffic; never silently fall back to direct. This module is the single
// source of truth for that decision and is fully unit-tested.

import {
  type ProxyManagerState,
  type ProbeResult,
  type HealthState,
  type DisplayStatus,
  FAILURE_THRESHOLD
} from "./types.js";

export interface RequestDecision {
  /** True => cancel the network request (block). */
  cancel: boolean;
  /** Stable reason for diagnostics / popup messaging. */
  reason:
    | "allow-direct" // proxy off, kill-switch off: ordinary direct browsing
    | "allow-proxied" // proxy on + healthy: routed through proxy
    | "block-killswitch-no-proxy" // kill-switch on but no usable proxy
    | "block-health-failing"; // proxy enabled but health check failing
}

/**
 * Decide whether to cancel a request given the current state. PURE.
 *
 * Rules:
 *   - If the kill-switch is OFF and the proxy is OFF → allow (plain direct).
 *   - If the proxy is ON and health is "healthy" → allow (it will be proxied).
 *   - Otherwise, whenever protection is expected (kill-switch on, or proxy on
 *     but unhealthy) → CANCEL. Never fall back to direct.
 *
 * The function deliberately treats "unknown" health as not-yet-safe: with the
 * kill-switch engaged, unknown blocks (fail-closed); only an explicit healthy
 * state permits proxied traffic.
 */
export function decideRequest(state: ProxyManagerState): RequestDecision {
  const hasProxy = state.proxyEnabled && state.proxy !== null;

  // Plain direct browsing: nothing engaged, nothing to protect.
  if (!state.killSwitch && !state.proxyEnabled) {
    return { cancel: false, reason: "allow-direct" };
  }

  // Proxy engaged and demonstrably healthy: allow (the onRequest handler routes
  // it through the proxy; DNS goes remote via proxyDNS).
  if (hasProxy && state.health === "healthy") {
    return { cancel: false, reason: "allow-proxied" };
  }

  // From here, protection is expected but not currently guaranteed.
  if (!hasProxy) {
    // Kill-switch on but no usable proxy configured/enabled → block.
    return { cancel: true, reason: "block-killswitch-no-proxy" };
  }

  // Proxy enabled but not healthy (failing/degraded/unknown) → block.
  return { cancel: true, reason: "block-health-failing" };
}

/**
 * Advance health given the previous state and a fresh probe result. PURE.
 *
 * Transitions:
 *   - "ok"      → healthy, failure counter reset to 0.
 *   - "fail"    → increment failures; degraded until the threshold, then failing.
 *   - "unknown" → preserve health but DO NOT clear failures (no positive signal).
 *
 * Returns the next health and the next consecutive-failure count.
 */
export function nextHealthState(
  prev: Pick<ProxyManagerState, "health" | "consecutiveFailures">,
  probe: ProbeResult
): { health: HealthState; consecutiveFailures: number } {
  if (probe === "ok") {
    return { health: "healthy", consecutiveFailures: 0 };
  }
  if (probe === "fail") {
    const failures = prev.consecutiveFailures + 1;
    const health: HealthState =
      failures >= FAILURE_THRESHOLD ? "failing" : "degraded";
    return { health, consecutiveFailures: failures };
  }
  // unknown: keep prior failure count; if we had never succeeded, stay unknown.
  const health: HealthState =
    prev.health === "healthy" ? "healthy" : prev.health === "unknown" ? "unknown" : prev.health;
  return { health, consecutiveFailures: prev.consecutiveFailures };
}

/**
 * Map state to the user-facing status shown in the popup. PURE.
 *   - protected: proxy on + healthy (and not blocking).
 *   - blocked:   fail-closed is actively cancelling traffic.
 *   - leaky:     proxy off but kill-switch also off (ordinary direct — only
 *                "leaky" relative to an expectation of proxying; surfaced so the
 *                user is never misled into thinking they are protected).
 *   - direct:    explicit direct with nothing engaged.
 */
export function displayStatus(state: ProxyManagerState): DisplayStatus {
  const decision = decideRequest(state);
  if (decision.cancel) return "blocked";
  const hasProxy = state.proxyEnabled && state.proxy !== null;
  if (hasProxy && state.health === "healthy") return "protected";
  // not cancelling and not proxied => plain direct browsing
  return state.killSwitch ? "leaky" : "direct";
}

/** Validate a host:port pair for a proxy config. PURE. */
export function isValidEndpoint(host: string, port: number): boolean {
  if (typeof host !== "string" || host.trim().length === 0) return false;
  if (!Number.isInteger(port) || port < 1 || port > 65535) return false;
  // Reject obvious whitespace / scheme prefixes; host should be a bare host.
  if (/\s/.test(host) || host.includes("://")) return false;
  return true;
}
