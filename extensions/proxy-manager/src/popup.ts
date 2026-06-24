// SPDX-License-Identifier: MPL-2.0
// OpenBook Proxy — popup controller. Browser glue; pure logic is imported.

import { type ProxyManagerState, type ProxyConfig, type ProxyType } from "./types.js";
import { displayStatus, isValidCheckUrl, isValidEndpoint } from "./failclosed.js";
import { webrtcPolicyFor, ipv6WarningFor } from "./leakcontrols.js";

function byId<T extends HTMLElement = HTMLElement>(id: string): T | null {
  return document.getElementById(id) as T | null;
}
function setText(id: string, text: string): void {
  const el = byId(id);
  if (el) el.textContent = text;
}

async function send<T = unknown>(msg: Record<string, unknown>): Promise<T> {
  return browser.runtime.sendMessage(msg) as Promise<T>;
}

function render(state: ProxyManagerState): void {
  // Status pill.
  const status = displayStatus(state);
  const statusEl = byId("status");
  if (statusEl) statusEl.className = `status status-${status}`;
  const statusText: Record<string, string> = {
    protected: "Protected (proxied)",
    blocked:
      state.proxyEnabled && state.proxy !== null && !state.proxy.checkUrl
        ? "Blocked (fail-closed) — set a health-check URL so the proxy can be proven healthy"
        : "Blocked (fail-closed)",
    leaky: "No proxy — blocked by kill-switch",
    direct: "Direct (no proxy)"
  };
  setText("status-text", statusText[status]);

  // Form fields.
  if (state.proxy) {
    const typeSel = byId<HTMLSelectElement>("type");
    if (typeSel) typeSel.value = state.proxy.type;
    const host = byId<HTMLInputElement>("host");
    if (host) host.value = state.proxy.host;
    const port = byId<HTMLInputElement>("port");
    if (port) port.value = String(state.proxy.port);
    const checkUrl = byId<HTMLInputElement>("checkurl");
    if (checkUrl) checkUrl.value = state.proxy.checkUrl ?? "";
  }
  const enabled = byId<HTMLInputElement>("enabled");
  if (enabled) enabled.checked = state.proxyEnabled;
  const ks = byId<HTMLInputElement>("killswitch");
  if (ks) ks.checked = state.killSwitch;

  // Leak controls summary (derived from pure helpers).
  const webrtc = webrtcPolicyFor(state);
  setText(
    "ctl-webrtc",
    `WebRTC: ${webrtc.peerConnectionEnabled ? "default" : "peer connections disabled (no IP leak)"}`
  );
  const isSocks = state.proxy?.type === "socks" || state.proxy?.type === "socks4";
  setText(
    "ctl-dns",
    `DNS: ${isSocks && state.proxy?.proxyDNS ? "resolved at proxy (proxyDNS)" : "local (pref-backed for SOCKS)"}`
  );
  // We do not know the tunnel's v6 coverage from the browser; assume v4-only
  // for a SOCKS endpoint unless told otherwise -> warn conservatively.
  const v6 = ipv6WarningFor(state, false);
  setText("ctl-ipv6", `IPv6: ${v6.warn ? "may bypass proxy — disable via policy" : "ok"}`);
  const ipv6Banner = byId("ipv6-warning");
  if (ipv6Banner) {
    ipv6Banner.hidden = !v6.warn;
    ipv6Banner.textContent = v6.message;
  }
  setText(
    "ctl-failclosed",
    `Fail-closed: ${state.killSwitch ? "ON" : "off"} — health ${state.health}`
  );
}

async function refresh(): Promise<void> {
  const res = await send<{ ok: boolean; state: ProxyManagerState }>({ cmd: "getState" });
  if (res.ok) render(res.state);
}

async function onSave(ev: Event): Promise<void> {
  ev.preventDefault();
  const type = (byId<HTMLSelectElement>("type")?.value ?? "socks") as ProxyType;
  const host = byId<HTMLInputElement>("host")?.value.trim() ?? "";
  const port = Number(byId<HTMLInputElement>("port")?.value ?? "0");
  const checkUrlRaw = byId<HTMLInputElement>("checkurl")?.value.trim() ?? "";
  const proxyEnabled = byId<HTMLInputElement>("enabled")?.checked ?? false;

  if (proxyEnabled && !isValidEndpoint(host, port)) {
    setText("form-error", "Enter a valid host and a port between 1 and 65535.");
    return;
  }
  if (checkUrlRaw.length > 0 && !isValidCheckUrl(checkUrlRaw)) {
    setText("form-error", "The health-check URL must be a valid https:// URL without credentials or a #fragment.");
    return;
  }
  setText("form-error", "");

  const proxy: ProxyConfig | null =
    host.length > 0 && port > 0
      ? {
          type,
          host,
          port,
          proxyDNS: type === "socks" || type === "socks4",
          ...(checkUrlRaw.length > 0 ? { checkUrl: checkUrlRaw } : {})
        }
      : null;

  const res = await send<{ ok: boolean; state: ProxyManagerState }>({
    cmd: "setProxy",
    proxy,
    proxyEnabled
  });
  if (res.ok) render(res.state);
}

async function onKillSwitch(): Promise<void> {
  const killSwitch = byId<HTMLInputElement>("killswitch")?.checked ?? true;
  const res = await send<{ ok: boolean; state: ProxyManagerState }>({
    cmd: "setKillSwitch",
    killSwitch
  });
  if (res.ok) render(res.state);
}

async function onProbe(): Promise<void> {
  const res = await send<{ ok: boolean; state: ProxyManagerState }>({ cmd: "probeNow" });
  if (res.ok) render(res.state);
}

function init(): void {
  byId<HTMLFormElement>("proxy-form")?.addEventListener("submit", (e) => {
    void onSave(e);
  });
  byId<HTMLInputElement>("killswitch")?.addEventListener("change", () => {
    void onKillSwitch();
  });
  byId<HTMLButtonElement>("probe")?.addEventListener("click", () => {
    void onProbe();
  });
  void refresh();
}

document.addEventListener("DOMContentLoaded", init);
