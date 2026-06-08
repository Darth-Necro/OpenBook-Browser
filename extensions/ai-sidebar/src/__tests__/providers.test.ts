// SPDX-License-Identifier: MPL-2.0
// Unit tests: provider streaming parsers + the BYOK egress flag. Uses an
// injected fake fetch; no real network.

import { LocalOllamaProvider } from '../providers/ollama.js';
import { BringYourOwnKeyProvider } from '../providers/byok.js';
import { type CompletionRequest } from '../providers/provider.js';

/** Build a fake fetch returning a streamed body from the given chunks. */
function fakeFetch(chunks: string[], ok = true, status = 200): typeof fetch {
  return (async () => {
    const encoder = new TextEncoder();
    let i = 0;
    const body = {
      getReader() {
        return {
          read(): Promise<{ value?: Uint8Array; done: boolean }> {
            if (i < chunks.length) {
              const value = encoder.encode(chunks[i++]);
              return Promise.resolve({ value, done: false });
            }
            return Promise.resolve({ value: undefined, done: true });
          }
        };
      }
    };
    return { ok, status, body } as unknown as Response;
  }) as unknown as typeof fetch;
}

async function collect(it: AsyncIterable<string> | Promise<string>): Promise<string> {
  if (typeof (it as AsyncIterable<string>)[Symbol.asyncIterator] === 'function') {
    let out = '';
    for await (const piece of it as AsyncIterable<string>) out += piece;
    return out;
  }
  return it as Promise<string>;
}

const req: CompletionRequest = {
  messages: [
    { role: 'system', content: 'sys' },
    { role: 'user', content: 'hi' }
  ]
};

describe('LocalOllamaProvider', () => {
  it('is local-only', () => {
    expect(new LocalOllamaProvider().sendsDataOffDevice).toBe(false);
  });

  it('parses newline-delimited JSON and concatenates content', async () => {
    const f = fakeFetch([
      '{"message":{"role":"assistant","content":"Hel"}}\n',
      '{"message":{"role":"assistant","content":"lo"}}\n{"done":true}\n'
    ]);
    const p = new LocalOllamaProvider('llama3.1', 'http://localhost:11434', f);
    expect(await collect(p.complete(req))).toBe('Hello');
  });

  it('throws on a non-ok response', async () => {
    const p = new LocalOllamaProvider('m', 'http://localhost:11434', fakeFetch([], false, 500));
    await expect(collect(p.complete(req))).rejects.toThrow(/500/);
  });
});

describe('BringYourOwnKeyProvider', () => {
  it('flags off-device egress', () => {
    const p = new BringYourOwnKeyProvider('https://api.x/v1', 'k', 'm', fakeFetch([]));
    expect(p.sendsDataOffDevice).toBe(true);
  });

  it('requires url, key, model', () => {
    expect(() => new BringYourOwnKeyProvider('', 'k', 'm')).toThrow();
    expect(() => new BringYourOwnKeyProvider('u', '', 'm')).toThrow();
    expect(() => new BringYourOwnKeyProvider('u', 'k', '')).toThrow();
  });

  it('parses SSE data lines and stops at [DONE]', async () => {
    const f = fakeFetch([
      'data: {"choices":[{"delta":{"content":"Hi"}}]}\n',
      'data: {"choices":[{"delta":{"content":" there"}}]}\n',
      'data: [DONE]\n'
    ]);
    const p = new BringYourOwnKeyProvider('https://api.x/v1', 'sk', 'gpt', f);
    expect(await collect(p.complete(req))).toBe('Hi there');
  });
});
