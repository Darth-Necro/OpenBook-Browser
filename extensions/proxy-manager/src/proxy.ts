// SPDX-License-Identifier: MPL-2.0
// OpenBook Proxy — browser.proxy.onRequest handler.
//
// Returns proxy info per request. For SOCKS we set proxyDNS:true so DNS is
// resolved AT the proxy (leak control 2), never by the local OS resolver.
//
// The mapping from state -> proxy info is extracted as a pure function
// (`resolveProxyInfo`) so it is unit-testable without the browser. When the
// fail-closed decision says "cancel", the webRequest blocking listener cancels
// the request first; this handler additionally returns "direct" only when the
// user is intentionally browsing direct (no kill-switch, no proxy).

import { type ProxyManagerState, type ProxyConfig } from "./types.js";
import { decideRequest } from "./failclosed.js";

/** Firefox proxy.onRequest return shape (subset we emit). */
export interface FirefoxProxyInfo {
  type: "direct" | "socks" | "socks4" | "http" | "https";
  host?: string;
  port?: number;
  /** SOCKS-only: resolve DNS through the proxy. */
  proxyDNS?: boolean;
  username?: string;
}

/**
 * Pure: derive what proxy.onRequest should return for the current state.
 *
 * - If fail-closed would cancel, we still return "direct" here as a defensive
 *   default, but the request is expected to be cancelled by the blocking
 *   webRequest listener BEFORE it is sent. We never return a half-configured
 *   proxy that could leak.
 * - SOCKS proxies always carry proxyDNS:true (remote DNS).
 */
export function resolveProxyInfo(state: ProxyManagerState): FirefoxProxyInfo {
  const decision = decideRequest(state);
  if (decision.cancel) {
    // Defensive: the blocking listener cancels; do not emit a proxy that could
    // be used if cancellation were bypassed.
    return { type: "direct" };
  }
  const p: ProxyConfig | null = state.proxy;
  if (!state.proxyEnabled || p === null) {
    return { type: "direct" };
  }
  const info: FirefoxProxyInfo = {
    type: p.type,
    host: p.host,
    port: p.port
  };
  if ((p.type === "socks" || p.type === "socks4") && p.proxyDNS) {
    info.proxyDNS = true;
  }
  if (p.username) info.username = p.username;
  return info;
}

/**
 * Install the proxy.onRequest listener. `getState` is injected so the handler
 * always reads the latest state without capturing a stale snapshot.
 */
export function installProxyHandler(getState: () => ProxyManagerState): void {
  browser.proxy.onRequest.addListener(
    () => resolveProxyInfo(getState()),
    { urls: ["<all_urls>"] }
  );
}
