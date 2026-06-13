// SPDX-License-Identifier: MPL-2.0
// Unit tests for the pure leak-control helpers (WebRTC, IPv6) and proxy info.

import { resolveProxyInfo } from '../proxy';
import { webrtcPolicyFor, ipv6WarningFor } from '../leakcontrols';
import { type ProxyManagerState, type ProxyConfig, DEFAULT_STATE } from '../types';

const socks: ProxyConfig = { type: 'socks', host: '127.0.0.1', port: 1080, proxyDNS: true };
const http: ProxyConfig = { type: 'http', host: 'p.example', port: 3128, proxyDNS: false };

function state(o: Partial<ProxyManagerState>): ProxyManagerState {
  return { ...DEFAULT_STATE, ...o };
}

describe('resolveProxyInfo (DNS leak control)', () => {
  it('returns proxyDNS:true for an enabled+healthy SOCKS proxy', () => {
    const info = resolveProxyInfo(
      state({ proxyEnabled: true, proxy: socks, health: 'healthy' })
    );
    expect(info.type).toBe('socks');
    expect(info.host).toBe('127.0.0.1');
    expect(info.port).toBe(1080);
    expect(info.proxyDNS).toBe(true);
  });

  it('does not set proxyDNS for HTTP proxies', () => {
    const info = resolveProxyInfo(
      state({ proxyEnabled: true, proxy: http, health: 'healthy' })
    );
    expect(info.type).toBe('http');
    expect(info.proxyDNS).toBeUndefined();
  });

  it('returns direct (never a half-config) when fail-closed would block', () => {
    // proxy enabled but failing => decideRequest cancels => defensive direct.
    const info = resolveProxyInfo(
      state({ proxyEnabled: true, proxy: socks, health: 'failing' })
    );
    expect(info.type).toBe('direct');
    expect(info.host).toBeUndefined();
  });

  it('returns direct when proxy disabled and kill-switch off', () => {
    const info = resolveProxyInfo(state({ proxyEnabled: false, killSwitch: false }));
    expect(info.type).toBe('direct');
  });

  // The health probe must MEASURE the proxy path: it is routed through the
  // configured proxy even while health is unproven (otherwise the probe tests
  // the direct path and health is meaningless), and it can never go direct.
  it('routes the active probe THROUGH the proxy even when health is failing', () => {
    const info = resolveProxyInfo(
      state({ proxyEnabled: true, proxy: socks, health: 'failing' }),
      { isProbe: true }
    );
    expect(info.type).toBe('socks');
    expect(info.host).toBe('127.0.0.1');
    expect(info.proxyDNS).toBe(true);
  });

  it('probe flag changes nothing when no proxy is configured (no direct leak)', () => {
    const info = resolveProxyInfo(
      state({ proxyEnabled: false, killSwitch: true, proxy: null }),
      { isProbe: true }
    );
    expect(info.type).toBe('direct'); // and the blocking listener cancels it
  });
});

describe('webrtcPolicyFor (WebRTC leak control)', () => {
  it('disables peer connections when a proxy is active', () => {
    const p = webrtcPolicyFor(state({ proxyEnabled: true, proxy: socks, health: 'healthy' }));
    expect(p.peerConnectionEnabled).toBe(false);
    expect(p.webRTCIPHandlingPolicy).toBe('disable_non_proxied_udp');
  });

  it('disables peer connections when the kill-switch is on even without a proxy', () => {
    const p = webrtcPolicyFor(state({ killSwitch: true, proxyEnabled: false }));
    expect(p.peerConnectionEnabled).toBe(false);
  });

  it('leaves WebRTC at default when nothing is engaged', () => {
    const p = webrtcPolicyFor(state({ killSwitch: false, proxyEnabled: false }));
    expect(p.peerConnectionEnabled).toBe(true);
    expect(p.webRTCIPHandlingPolicy).toBe('default');
  });
});

describe('ipv6WarningFor (IPv6 leak control)', () => {
  it('warns when a proxy is active but the tunnel is v4-only', () => {
    const w = ipv6WarningFor(state({ proxyEnabled: true, proxy: socks, health: 'healthy' }), false);
    expect(w.warn).toBe(true);
    expect(w.message).toMatch(/IPv6/);
  });

  it('does not warn when the tunnel covers IPv6', () => {
    const w = ipv6WarningFor(state({ proxyEnabled: true, proxy: socks }), true);
    expect(w.warn).toBe(false);
  });

  it('does not warn when no proxy is active', () => {
    const w = ipv6WarningFor(state({ proxyEnabled: false }), false);
    expect(w.warn).toBe(false);
  });
});
