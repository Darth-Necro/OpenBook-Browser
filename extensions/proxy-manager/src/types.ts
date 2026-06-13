// SPDX-License-Identifier: MPL-2.0
// OpenBook Proxy — shared types for the proxy + leak-control state machine.

/** Proxy transport. SOCKS5 is preferred because it can carry remote DNS. */
export type ProxyType = "socks" | "socks4" | "http" | "https";

/** User-configured proxy endpoint. */
export interface ProxyConfig {
  type: ProxyType;
  host: string;
  port: number;
  /** Resolve DNS at the proxy (SOCKS only). Backs proxyDNS in onRequest. */
  proxyDNS: boolean;
  /** Optional username for proxy auth (passed to Firefox proxy auth, not here). */
  username?: string;
  /**
   * User-chosen HTTPS URL fetched THROUGH the proxy to prove the path is
   * alive. OpenBook ships no default endpoint (shipping one would hardcode
   * unsolicited egress to a third party — invariant 1); without it the proxy
   * cannot be proven healthy and fail-closed keeps blocking.
   */
  checkUrl?: string;
}

/** Health-check probe outcome. */
export type ProbeResult = "ok" | "fail" | "unknown";

/** Derived health of the proxy path. */
export type HealthState = "healthy" | "degraded" | "failing" | "unknown";

/**
 * The full runtime state the fail-closed decision depends on. Kept plain and
 * serializable so it round-trips through storage and is trivially testable.
 */
export interface ProxyManagerState {
  /** Master on/off for routing through the configured proxy. */
  proxyEnabled: boolean;
  /** The configured endpoint, or null if unconfigured. */
  proxy: ProxyConfig | null;
  /**
   * Fail-closed kill-switch. When true, traffic is blocked unless the proxy is
   * enabled AND healthy. This is the user-facing "never leak" guarantee.
   */
  killSwitch: boolean;
  /** Latest derived health of the proxy path. */
  health: HealthState;
  /** Consecutive probe failures (drives degraded -> failing). */
  consecutiveFailures: number;
}

/** Public status shown in the popup. */
export type DisplayStatus = "protected" | "blocked" | "leaky" | "direct";

/** IPv6 handling preference (leak control 3). */
export interface IPv6Preference {
  /** Whether the active tunnel/proxy is known to carry IPv6. */
  tunnelCoversIPv6: boolean;
}

export const DEFAULT_STATE: ProxyManagerState = {
  proxyEnabled: false,
  proxy: null,
  killSwitch: true, // fail-closed by default: safer to block than to leak
  health: "unknown",
  consecutiveFailures: 0
};

/** Failures tolerated before health flips from degraded to failing. */
export const FAILURE_THRESHOLD = 2;
