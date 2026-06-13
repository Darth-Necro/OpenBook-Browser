// SPDX-License-Identifier: MPL-2.0
// Unit tests: off-by-default registry => no provider => no possible network.

import {
  getActiveProvider,
  isLoopbackBaseUrl,
  networkAllowed,
  DEFAULT_SETTINGS,
  type AssistantSettings
} from '../registry.js';
import { LocalOllamaProvider } from '../providers/ollama.js';
import { BringYourOwnKeyProvider } from '../providers/byok.js';

function settings(o: Partial<AssistantSettings>): AssistantSettings {
  return { ...DEFAULT_SETTINGS, ...o };
}

describe('off-by-default invariant (§7)', () => {
  it('default settings expose NO active provider', () => {
    expect(getActiveProvider(DEFAULT_SETTINGS)).toBeNull();
    expect(networkAllowed(DEFAULT_SETTINGS)).toBe(false);
  });

  it('defaults are enabled=false, provider=null, acknowledgedEgress=false', () => {
    expect(DEFAULT_SETTINGS.enabled).toBe(false);
    expect(DEFAULT_SETTINGS.provider).toBeNull();
    expect(DEFAULT_SETTINGS.acknowledgedEgress).toBe(false);
  });

  it('returns null when a provider is configured but the assistant is disabled', () => {
    const s = settings({
      enabled: false,
      provider: { id: 'ollama', model: 'llama3.1' }
    });
    expect(getActiveProvider(s)).toBeNull();
    expect(networkAllowed(s)).toBe(false);
  });

  it('returns null when enabled but no provider configured', () => {
    expect(getActiveProvider(settings({ enabled: true, provider: null }))).toBeNull();
  });

  it('returns null for an unknown provider id', () => {
    const s = settings({ enabled: true, provider: { id: 'evilcorp', model: 'x' } });
    expect(getActiveProvider(s)).toBeNull();
  });
});

describe('local Ollama activation', () => {
  it('builds a local provider when enabled + configured (no egress)', () => {
    const s = settings({
      enabled: true,
      provider: { id: 'ollama', model: 'llama3.1' }
    });
    const p = getActiveProvider(s, (() => {}) as unknown as typeof fetch);
    expect(p).toBeInstanceOf(LocalOllamaProvider);
    expect(p?.sendsDataOffDevice).toBe(false);
    expect(networkAllowed(s)).toBe(true);
  });

  // "Local only" must be provably local: a remote baseUrl on the ollama
  // provider would ship page content off-device while labeled local and
  // without the egress acknowledgement. The registry must refuse it.
  it('refuses a NON-loopback ollama baseUrl (mislabeled egress)', () => {
    const s = settings({
      enabled: true,
      provider: { id: 'ollama', model: 'llama3.1', baseUrl: 'https://collector.example' }
    });
    expect(getActiveProvider(s)).toBeNull();
    expect(networkAllowed(s)).toBe(false);
  });

  it('accepts explicit loopback baseUrls', () => {
    for (const baseUrl of [
      'http://localhost:11434',
      'http://127.0.0.1:11434',
      'http://[::1]:11434'
    ]) {
      const s = settings({ enabled: true, provider: { id: 'ollama', model: 'm', baseUrl } });
      expect(getActiveProvider(s, (() => {}) as unknown as typeof fetch)).toBeInstanceOf(
        LocalOllamaProvider
      );
    }
  });
});

describe('isLoopbackBaseUrl', () => {
  it('accepts loopback hosts only', () => {
    expect(isLoopbackBaseUrl('http://localhost:11434')).toBe(true);
    expect(isLoopbackBaseUrl('http://127.0.0.1:11434')).toBe(true);
    expect(isLoopbackBaseUrl('http://127.8.9.10:80')).toBe(true);
    expect(isLoopbackBaseUrl('http://[::1]:11434')).toBe(true);
  });
  it('rejects remote, malformed, and non-http(s) URLs', () => {
    expect(isLoopbackBaseUrl('https://collector.example')).toBe(false);
    expect(isLoopbackBaseUrl('http://192.168.1.10:11434')).toBe(false);
    expect(isLoopbackBaseUrl('http://localhost.evil.example')).toBe(false);
    expect(isLoopbackBaseUrl('file:///etc/passwd')).toBe(false);
    expect(isLoopbackBaseUrl('not a url')).toBe(false);
  });
});

describe('BYOK activation requires explicit egress acknowledgement', () => {
  const baseCfg = {
    id: 'byok',
    model: 'gpt-4o-mini',
    baseUrl: 'https://api.example.com/v1',
    apiKey: 'sk-test'
  };

  it('is null without acknowledgedEgress even when fully configured + enabled', () => {
    const s = settings({ enabled: true, provider: baseCfg, acknowledgedEgress: false });
    expect(getActiveProvider(s)).toBeNull();
  });

  it('builds a BYOK provider only with the egress ack', () => {
    const s = settings({ enabled: true, provider: baseCfg, acknowledgedEgress: true });
    const p = getActiveProvider(s, (() => {}) as unknown as typeof fetch);
    expect(p).toBeInstanceOf(BringYourOwnKeyProvider);
    expect(p?.sendsDataOffDevice).toBe(true);
  });

  it('is null when BYOK lacks required fields (no key / url / model)', () => {
    expect(
      getActiveProvider(
        settings({
          enabled: true,
          acknowledgedEgress: true,
          provider: { id: 'byok', model: '', baseUrl: 'https://x', apiKey: 'k' }
        })
      )
    ).toBeNull();
    expect(
      getActiveProvider(
        settings({
          enabled: true,
          acknowledgedEgress: true,
          provider: { id: 'byok', model: 'm', baseUrl: '', apiKey: 'k' }
        })
      )
    ).toBeNull();
    expect(
      getActiveProvider(
        settings({
          enabled: true,
          acknowledgedEgress: true,
          provider: { id: 'byok', model: 'm', baseUrl: 'https://x', apiKey: '' }
        })
      )
    ).toBeNull();
  });
});
