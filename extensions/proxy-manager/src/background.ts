// SPDX-License-Identifier: MPL-2.0
// OpenBook Proxy — persistent background. Wires the four leak controls together.
//
//   1. WebRTC  — applyWebRtcPolicy(webrtcPolicyFor(state)) on every state change.
//   2. DNS     — proxyDNS:true via the proxy.onRequest handler (proxy.ts).
//   3. IPv6    — warning surfaced in the popup (ipv6WarningFor); pref-backed.
//   4. FAIL-CLOSED — a BLOCKING webRequest.onBeforeRequest listener that cancels
//      every request whenever decideRequest(state).cancel is true, plus a
//      periodic health-check that flips health and re-evaluates.
//
// Pure decisions live in failclosed.ts / leakcontrols.ts / proxy.ts.

import {
  type ProxyManagerState,
  type ProxyConfig,
  type ProbeResult,
  DEFAULT_STATE
} from "./types.js";
import { decideRequest, isProbeRequest, nextHealthState } from "./failclosed.js";
import { installProxyHandler } from "./proxy.js";
import { applyWebRtcPolicy, webrtcPolicyFor } from "./leakcontrols.js";

const STORAGE_KEY = "proxyManagerState";
/** Health-check cadence (ms). */
const PROBE_INTERVAL_MS = 15_000;
/** Probe timeout (ms). */
const PROBE_TIMEOUT_MS = 5_000;

let state: ProxyManagerState = { ...DEFAULT_STATE };
let probeTimer: ReturnType<typeof setInterval> | null = null;
/**
 * Exact URL of the probe currently in flight (carries a fresh random nonce),
 * or null. The blocking listener exempts ONLY this URL from extension context
 * (isProbeRequest) — and the proxy handler still routes it through the proxy.
 */
let activeProbeUrl: string | null = null;

function getState(): ProxyManagerState {
  return state;
}

async function loadState(): Promise<void> {
  try {
    const got = await browser.storage.local.get(STORAGE_KEY);
    const saved = got[STORAGE_KEY] as Partial<ProxyManagerState> | undefined;
    if (saved) state = { ...DEFAULT_STATE, ...saved };
  } catch {
    state = { ...DEFAULT_STATE };
  }
}

async function persist(): Promise<void> {
  try {
    await browser.storage.local.set({ [STORAGE_KEY]: state });
  } catch {
    /* storage failure must not open the gate; in-memory state still applies */
  }
}

/** Re-apply side effects that depend on state (WebRTC). */
async function applyState(): Promise<void> {
  await applyWebRtcPolicy(webrtcPolicyFor(state));
}

/**
 * Pure-ish classifier kept tiny: turn a fetch outcome into a ProbeResult.
 * Exported for tests via the failclosed/types modules is unnecessary; the
 * fetch itself is the only impure part.
 */
function classifyProbe(ok: boolean, errored: boolean): ProbeResult {
  if (errored) return "fail";
  return ok ? "ok" : "fail";
}

/**
 * Health-check: fetch the user-configured check URL THROUGH the proxy (the
 * proxy handler routes the active probe via the configured endpoint even
 * while health is unproven — that is the point of the probe). A failure means
 * the path is down → fail-closed blocks. Without a user-supplied checkUrl no
 * probe runs at all: health stays unproven and traffic stays blocked, and the
 * popup says why. OpenBook ships no default probe endpoint (invariant 1).
 */
async function runProbe(): Promise<void> {
  // Only probe when a proxy is enabled+configured; otherwise health is moot.
  if (!state.proxyEnabled || state.proxy === null) {
    return;
  }
  const checkUrl = state.proxy.checkUrl;
  if (!checkUrl) {
    return; // unprovable -> stays blocked; surfaced in the popup
  }
  if (activeProbeUrl !== null) {
    return; // one probe at a time; the nonce exemption is single-use
  }
  // Per-probe random nonce: makes the exempted URL unguessable by pages and
  // doubles as a cache-buster.
  const nonce = crypto.getRandomValues(new Uint32Array(4)).join("-");
  let probeUrl: string;
  try {
    const u = new URL(checkUrl);
    u.searchParams.set("openbook-probe", nonce);
    // The fragment is never sent over the wire, so Firefox's webRequest/proxy
    // details.url omits it — keeping it here would make activeProbeUrl differ
    // from details.url, the exact-match exemption would miss, and the probe
    // would cancel itself (health could never go healthy). Drop it.
    u.hash = "";
    probeUrl = u.toString();
  } catch {
    return; // invalid stored URL: unprovable -> stays blocked
  }
  let ok = false;
  let errored = false;
  const controller = new AbortController();
  const t = setTimeout(() => controller.abort(), PROBE_TIMEOUT_MS);
  activeProbeUrl = probeUrl;
  try {
    const res = await fetch(probeUrl, {
      method: "HEAD",
      cache: "no-store",
      signal: controller.signal
    });
    ok = res.ok;
  } catch {
    errored = true;
  } finally {
    activeProbeUrl = null;
    clearTimeout(t);
  }
  const probe = classifyProbe(ok, errored);
  const next = nextHealthState(state, probe);
  state = { ...state, health: next.health, consecutiveFailures: next.consecutiveFailures };
  await persist();
  await applyState();
}

function startProbeLoop(): void {
  if (probeTimer) clearInterval(probeTimer);
  probeTimer = setInterval(() => {
    void runProbe();
  }, PROBE_INTERVAL_MS);
}

// --- Fail-closed blocking listener (leak control 4) -------------------------

function onBeforeRequest(
  details: browser.webRequest._OnBeforeRequestDetails
): browser.webRequest.BlockingResponse {
  // The single deliberate exemption: the in-flight health probe (nonce URL,
  // extension context). It is exempt from CANCELLATION only — the proxy
  // handler still routes it through the proxy, so it cannot leak direct.
  if (isProbeRequest({ url: details.url, tabId: details.tabId }, activeProbeUrl)) {
    return { cancel: false };
  }
  const decision = decideRequest(state);
  return { cancel: decision.cancel };
}

function installFailClosed(): void {
  browser.webRequest.onBeforeRequest.addListener(
    onBeforeRequest,
    { urls: ["<all_urls>"] },
    ["blocking"]
  );
}

// --- Popup/messaging API ----------------------------------------------------

type PopupMessage =
  | { cmd: "getState" }
  | { cmd: "setProxy"; proxy: ProxyConfig | null; proxyEnabled: boolean }
  | { cmd: "setKillSwitch"; killSwitch: boolean }
  | { cmd: "probeNow" };

async function handleMessage(msg: PopupMessage): Promise<unknown> {
  switch (msg.cmd) {
    case "getState":
      return { ok: true, state };
    case "setProxy":
      state = {
        ...state,
        proxy: msg.proxy,
        proxyEnabled: msg.proxyEnabled,
        // Reset health on reconfiguration: must re-prove the path. Until the
        // next successful probe, fail-closed treats it as not-yet-safe.
        health: "unknown",
        consecutiveFailures: 0
      };
      await persist();
      await applyState();
      void runProbe();
      return { ok: true, state };
    case "setKillSwitch":
      state = { ...state, killSwitch: msg.killSwitch };
      await persist();
      await applyState();
      return { ok: true, state };
    case "probeNow":
      await runProbe();
      return { ok: true, state };
    default:
      return { ok: false, error: "invalid-request" };
  }
}

browser.runtime.onMessage.addListener((message: unknown) =>
  handleMessage(message as PopupMessage).catch((e: unknown) => ({
    ok: false,
    error: "internal",
    message: e instanceof Error ? e.message : String(e)
  }))
);

// --- Boot -------------------------------------------------------------------
// ORDER MATTERS (invariant 2): the blocking listener and the proxy handler are
// registered SYNCHRONOUSLY at module top level, BEFORE any await. The default
// in-memory state is fail-closed (killSwitch on), so requests racing extension
// startup (e.g. session restore) are blocked, never allowed direct. Loading
// persisted state happens after — by then the gate is already up.

installProxyHandler(getState, (details) => isProbeRequest(details, activeProbeUrl));
installFailClosed();

async function init(): Promise<void> {
  await loadState();
  await applyState();
  startProbeLoop();
}

void init();
