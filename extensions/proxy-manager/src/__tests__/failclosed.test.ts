// SPDX-License-Identifier: MPL-2.0
// Unit tests for the fail-closed decision core + health transitions.

import {
  decideRequest,
  isProbeRequest,
  isValidCheckUrl,
  nextHealthState,
  displayStatus,
  isValidEndpoint
} from '../failclosed';
import {
  type ProxyManagerState,
  type ProxyConfig,
  DEFAULT_STATE,
  FAILURE_THRESHOLD
} from '../types';

const proxy: ProxyConfig = {
  type: 'socks',
  host: '127.0.0.1',
  port: 1080,
  proxyDNS: true
};

function state(overrides: Partial<ProxyManagerState>): ProxyManagerState {
  return { ...DEFAULT_STATE, ...overrides };
}

describe('decideRequest (fail-closed)', () => {
  it('ALLOWS plain direct when kill-switch off and proxy off', () => {
    const d = decideRequest(state({ killSwitch: false, proxyEnabled: false }));
    expect(d.cancel).toBe(false);
    expect(d.reason).toBe('allow-direct');
  });

  it('ALLOWS only when proxy enabled AND healthy AND configured', () => {
    const d = decideRequest(
      state({ proxyEnabled: true, proxy, health: 'healthy', killSwitch: true })
    );
    expect(d.cancel).toBe(false);
    expect(d.reason).toBe('allow-proxied');
  });

  it('BLOCKS when kill-switch engaged but no proxy configured', () => {
    const d = decideRequest(state({ killSwitch: true, proxyEnabled: false, proxy: null }));
    expect(d.cancel).toBe(true);
    expect(d.reason).toBe('block-killswitch-no-proxy');
  });

  it('BLOCKS when kill-switch engaged and proxyEnabled but proxy is null', () => {
    const d = decideRequest(state({ killSwitch: true, proxyEnabled: true, proxy: null }));
    expect(d.cancel).toBe(true);
    expect(d.reason).toBe('block-killswitch-no-proxy');
  });

  it('BLOCKS when proxy enabled but health is failing (no silent direct)', () => {
    const d = decideRequest(state({ proxyEnabled: true, proxy, health: 'failing' }));
    expect(d.cancel).toBe(true);
    expect(d.reason).toBe('block-health-failing');
  });

  it('BLOCKS when proxy enabled but health is unknown (fail-closed on uncertainty)', () => {
    const d = decideRequest(state({ proxyEnabled: true, proxy, health: 'unknown' }));
    expect(d.cancel).toBe(true);
    expect(d.reason).toBe('block-health-failing');
  });

  it('BLOCKS when proxy enabled but health is degraded', () => {
    const d = decideRequest(state({ proxyEnabled: true, proxy, health: 'degraded' }));
    expect(d.cancel).toBe(true);
  });

  it('does not fall back to direct on tunnel loss (regression of invariant 2)', () => {
    // Was healthy and proxied, then health drops to failing => must block, not direct.
    const healthy = decideRequest(state({ proxyEnabled: true, proxy, health: 'healthy' }));
    const dropped = decideRequest(state({ proxyEnabled: true, proxy, health: 'failing' }));
    expect(healthy.cancel).toBe(false);
    expect(dropped.cancel).toBe(true);
  });
});

describe('nextHealthState', () => {
  it('ok -> healthy and resets failures', () => {
    expect(nextHealthState({ health: 'failing', consecutiveFailures: 5 }, 'ok')).toEqual({
      health: 'healthy',
      consecutiveFailures: 0
    });
  });

  it('first fail -> degraded', () => {
    expect(nextHealthState({ health: 'healthy', consecutiveFailures: 0 }, 'fail')).toEqual({
      health: 'degraded',
      consecutiveFailures: 1
    });
  });

  it('reaching the threshold -> failing', () => {
    let s = { health: 'healthy' as const, consecutiveFailures: 0 };
    let cur: { health: string; consecutiveFailures: number } = s;
    for (let i = 0; i < FAILURE_THRESHOLD; i++) {
      cur = nextHealthState(cur as typeof s, 'fail');
    }
    expect(cur.consecutiveFailures).toBe(FAILURE_THRESHOLD);
    expect(cur.health).toBe('failing');
  });

  it('unknown preserves prior failure count and healthy/unknown health', () => {
    expect(nextHealthState({ health: 'healthy', consecutiveFailures: 0 }, 'unknown')).toEqual({
      health: 'healthy',
      consecutiveFailures: 0
    });
    expect(nextHealthState({ health: 'unknown', consecutiveFailures: 0 }, 'unknown')).toEqual({
      health: 'unknown',
      consecutiveFailures: 0
    });
    expect(nextHealthState({ health: 'degraded', consecutiveFailures: 1 }, 'unknown')).toEqual({
      health: 'degraded',
      consecutiveFailures: 1
    });
  });

  it('recovers: fail then ok returns to healthy', () => {
    const a = nextHealthState({ health: 'healthy', consecutiveFailures: 0 }, 'fail');
    const b = nextHealthState(a, 'ok');
    expect(b).toEqual({ health: 'healthy', consecutiveFailures: 0 });
  });
});

describe('displayStatus', () => {
  it('protected when proxied + healthy', () => {
    expect(displayStatus(state({ proxyEnabled: true, proxy, health: 'healthy' }))).toBe(
      'protected'
    );
  });
  it('blocked when fail-closed cancels', () => {
    expect(displayStatus(state({ proxyEnabled: true, proxy, health: 'failing' }))).toBe(
      'blocked'
    );
    expect(displayStatus(state({ killSwitch: true, proxyEnabled: false }))).toBe('blocked');
  });
  it('direct when nothing engaged', () => {
    expect(displayStatus(state({ killSwitch: false, proxyEnabled: false }))).toBe('direct');
  });
});

describe('isValidEndpoint', () => {
  it('accepts valid host:port', () => {
    expect(isValidEndpoint('127.0.0.1', 1080)).toBe(true);
    expect(isValidEndpoint('proxy.example.net', 9050)).toBe(true);
  });
  it('rejects bad ports', () => {
    expect(isValidEndpoint('127.0.0.1', 0)).toBe(false);
    expect(isValidEndpoint('127.0.0.1', 70000)).toBe(false);
    expect(isValidEndpoint('127.0.0.1', 1.5)).toBe(false);
  });
  it('rejects empty / scheme-prefixed / whitespace hosts', () => {
    expect(isValidEndpoint('', 1080)).toBe(false);
    expect(isValidEndpoint('   ', 1080)).toBe(false);
    expect(isValidEndpoint('socks://h', 1080)).toBe(false);
    expect(isValidEndpoint('a b', 1080)).toBe(false);
  });
});

describe('isValidCheckUrl (probe target validation)', () => {
  it('accepts plain https URLs', () => {
    expect(isValidCheckUrl('https://example.org/')).toBe(true);
    expect(isValidCheckUrl('https://status.myproxy.net/health')).toBe(true);
  });
  it('rejects http, credentials, and garbage', () => {
    expect(isValidCheckUrl('http://example.org/')).toBe(false); // downgradable
    expect(isValidCheckUrl('https://user:pw@example.org/')).toBe(false);
    expect(isValidCheckUrl('ftp://example.org/')).toBe(false);
    expect(isValidCheckUrl('not a url')).toBe(false);
    expect(isValidCheckUrl('')).toBe(false);
  });
  it('rejects a #fragment (it is stripped from the network details.url, so an', () => {
    // exact-match probe carrying one would cancel itself -> permanent block).
    expect(isValidCheckUrl('https://example.org/#ok')).toBe(false);
    expect(isValidCheckUrl('https://example.org/health#section')).toBe(false);
  });
});

describe('isProbeRequest (the single fail-closed exemption)', () => {
  const probeUrl = 'https://example.org/?openbook-probe=1-2-3-4';

  it('matches only the exact in-flight probe URL from extension context', () => {
    expect(isProbeRequest({ url: probeUrl, tabId: -1 }, probeUrl)).toBe(true);
  });
  it('never matches when no probe is in flight', () => {
    expect(isProbeRequest({ url: probeUrl, tabId: -1 }, null)).toBe(false);
  });
  it('never matches a tab-originated request (page cannot forge tabId -1)', () => {
    expect(isProbeRequest({ url: probeUrl, tabId: 7 }, probeUrl)).toBe(false);
  });
  it('never matches a different URL (nonce mismatch)', () => {
    expect(
      isProbeRequest({ url: 'https://example.org/?openbook-probe=other', tabId: -1 }, probeUrl)
    ).toBe(false);
  });
});
