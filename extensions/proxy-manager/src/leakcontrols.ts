// SPDX-License-Identifier: MPL-2.0
// OpenBook Proxy — leak controls 1 (WebRTC), 2 (DNS), 3 (IPv6).
//
// Control 4 (fail-closed) lives in failclosed.ts + background.ts.
//
// What is enforced WHERE:
//   1. WebRTC  — enforced by THIS extension via browser.privacy.network
//                (peerConnectionEnabled / webRTCIPHandlingPolicy).
//   2. DNS     — proxyDNS:true in proxy.onRequest (proxy.ts) for SOCKS, backed
//                by the `network.proxy.socks_remote_dns` pref set via
//                autoconfig/policy (NOT settable from an extension).
//   3. IPv6    — surfaced here as a warning + setting; the actual
//                `network.dns.disableIPv6` pref is set via autoconfig/policy.
//
// The pure helpers (`webrtcPolicyFor`, `ipv6WarningFor`) are unit-testable.

import { type ProxyManagerState } from "./types.js";

/** WebRTC enforcement we apply via browser.privacy.network. */
export interface WebRtcPolicy {
  /** false disables peer connections entirely (strongest, prevents ICE leak). */
  peerConnectionEnabled: boolean;
  /**
   * When peer connections remain enabled, force ICE to use only the proxy and
   * never expose host/local candidates. "disable_non_proxied_udp" is the
   * leak-safe mode when a proxy is active.
   */
  webRTCIPHandlingPolicy:
    | "default"
    | "default_public_interface_only"
    | "default_public_and_private_interfaces"
    | "disable_non_proxied_udp";
}

/**
 * Pure: decide the WebRTC policy for the current state.
 *
 * When a proxy is active we MUST NOT let ICE expose the real IP. The leak-safe
 * choice is to disable non-proxied UDP; the strongest is to disable peer
 * connections outright. We default to disabling peer connections whenever the
 * proxy is engaged (and keep the handling policy locked to the safe mode in
 * case another component re-enables peer connections).
 */
export function webrtcPolicyFor(state: ProxyManagerState): WebRtcPolicy {
  const proxyActive = state.proxyEnabled && state.proxy !== null;
  if (proxyActive || state.killSwitch) {
    return {
      peerConnectionEnabled: false,
      webRTCIPHandlingPolicy: "disable_non_proxied_udp"
    };
  }
  // No proxy and no kill-switch: leave WebRTC at the browser default. (Hardened
  // global defaults are still applied by autoconfig; this is the extension's
  // proxy-scoped behavior only.)
  return {
    peerConnectionEnabled: true,
    webRTCIPHandlingPolicy: "default"
  };
}

export interface IPv6Warning {
  /** True => warn the user that IPv6 may bypass a v4-only tunnel. */
  warn: boolean;
  message: string;
}

/**
 * Pure: produce an IPv6 leak warning. If a proxy/tunnel is active but is not
 * known to carry IPv6, native IPv6 connectivity can bypass it and leak the real
 * address. The extension surfaces the warning; the actual disable is the
 * `network.dns.disableIPv6` pref (autoconfig/policy backed).
 */
export function ipv6WarningFor(
  state: ProxyManagerState,
  tunnelCoversIPv6: boolean
): IPv6Warning {
  const proxyActive = state.proxyEnabled && state.proxy !== null;
  if (proxyActive && !tunnelCoversIPv6) {
    return {
      warn: true,
      message:
        "This proxy may be IPv4-only. Native IPv6 can bypass it and expose your real address. Disable IPv6 (network.dns.disableIPv6, set via OpenBook policy) for a v4-only proxy."
    };
  }
  return { warn: false, message: "" };
}

/** The pref name that backs DNS-through-proxy (documentation surface). */
export const SOCKS_REMOTE_DNS_PREF = "network.proxy.socks_remote_dns";
/** The pref name that backs IPv6 disabling (documentation surface). */
export const DISABLE_IPV6_PREF = "network.dns.disableIPv6";

/**
 * Apply the WebRTC policy via browser.privacy.network. Side-effecting; the
 * decision itself is `webrtcPolicyFor`.
 */
export async function applyWebRtcPolicy(policy: WebRtcPolicy): Promise<void> {
  // Both settings are best-effort: a setting may be locked by policy, in which
  // case set() rejects and we swallow (policy is the stronger control anyway).
  try {
    await browser.privacy.network.peerConnectionEnabled.set({
      value: policy.peerConnectionEnabled
    });
  } catch {
    /* locked by policy or unavailable */
  }
  try {
    // Not all builds expose this typed; guard defensively.
    const net = browser.privacy.network as unknown as {
      webRTCIPHandlingPolicy?: { set(d: { value: string }): Promise<void> };
    };
    if (net.webRTCIPHandlingPolicy) {
      await net.webRTCIPHandlingPolicy.set({
        value: policy.webRTCIPHandlingPolicy
      });
    }
  } catch {
    /* locked by policy or unavailable */
  }
}
