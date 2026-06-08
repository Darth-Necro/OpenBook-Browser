// SPDX-License-Identifier: MPL-2.0
// Unit tests for the vault native-messaging protocol + VaultClient.

import {
  VaultClient,
  evaluateSecretStrength,
  isVaultResponse,
  isErrorResponse,
  MIN_SOFTWARE_SECRET_LENGTH,
  DEFAULT_MAX_ATTEMPTS,
  type PortLike,
  type VaultResponse,
  type VaultRequest
} from '../protocol';

/**
 * A fake native port that records outgoing messages and lets the test drive
 * inbound responses + disconnects, mirroring browser.runtime.Port.
 */
class FakePort implements PortLike {
  sent: VaultRequest[] = [];
  disconnected = false;
  private msgListeners = new Set<(m: unknown) => void>();
  private discListeners = new Set<(p?: unknown) => void>();

  postMessage(message: unknown): void {
    this.sent.push(message as VaultRequest);
  }
  onMessage = {
    addListener: (cb: (m: unknown) => void) => {
      this.msgListeners.add(cb);
    },
    removeListener: (cb: (m: unknown) => void) => {
      this.msgListeners.delete(cb);
    }
  };
  onDisconnect = {
    addListener: (cb: (p?: unknown) => void) => {
      this.discListeners.add(cb);
    },
    removeListener: (cb: (p?: unknown) => void) => {
      this.discListeners.delete(cb);
    }
  };
  disconnect(): void {
    this.disconnected = true;
    for (const cb of this.discListeners) cb();
  }
  /** Test helper: deliver a response frame to the client. */
  emit(message: unknown): void {
    for (const cb of this.msgListeners) cb(message);
  }
  /** Test helper: simulate the host process exiting. */
  drop(): void {
    for (const cb of this.discListeners) cb();
  }
  get lastSent(): VaultRequest {
    return this.sent[this.sent.length - 1];
  }
}

function makeClient(port: FakePort, opts: { seq?: () => string } = {}) {
  let n = 0;
  return new VaultClient({
    portFactory: () => port,
    idGenerator: opts.seq ?? (() => `id-${++n}`),
    // 0 disables the per-request timer so tests that only inspect the outgoing
    // message (and never deliver a response) leave no pending real timeout.
    // The dedicated timeout test below constructs its own client.
    requestTimeoutMs: 0
  });
}

describe('isVaultResponse', () => {
  it('accepts a well-formed envelope', () => {
    expect(isVaultResponse({ id: 'x', ok: true })).toBe(true);
    expect(isVaultResponse({ id: 'x', ok: false, error: 'bad-secret' })).toBe(true);
  });
  it('rejects malformed values', () => {
    expect(isVaultResponse(null)).toBe(false);
    expect(isVaultResponse(undefined)).toBe(false);
    expect(isVaultResponse('nope')).toBe(false);
    expect(isVaultResponse({ id: 1, ok: true })).toBe(false); // id not string
    expect(isVaultResponse({ id: 'x' })).toBe(false); // missing ok
    expect(isVaultResponse({ ok: true })).toBe(false); // missing id
  });
});

describe('isErrorResponse', () => {
  it('discriminates on ok', () => {
    const ok: VaultResponse = { id: '1', ok: true, state: 'locked' };
    const err: VaultResponse = { id: '2', ok: false, error: 'bad-secret' };
    expect(isErrorResponse(ok)).toBe(false);
    expect(isErrorResponse(err)).toBe(true);
  });
});

describe('VaultClient request serialization', () => {
  it('serializes setup with acknowledgeNoRecovery and default maxAttempts', () => {
    const port = new FakePort();
    const client = makeClient(port);
    void client.setup({ secret: 'correct horse battery' });
    expect(port.lastSent).toMatchObject({
      type: 'setup',
      id: 'id-1',
      secret: 'correct horse battery',
      maxAttempts: DEFAULT_MAX_ATTEMPTS,
      acknowledgeNoRecovery: true
    });
  });

  it('honors an explicit maxAttempts', () => {
    const port = new FakePort();
    const client = makeClient(port);
    void client.setup({ secret: 'correct horse battery', maxAttempts: 3 });
    expect(port.lastSent.type).toBe('setup');
    expect((port.lastSent as { maxAttempts: number }).maxAttempts).toBe(3);
  });

  it('serializes unlock / lock / erase / status', () => {
    const port = new FakePort();
    const client = makeClient(port);
    void client.status();
    expect(port.lastSent).toMatchObject({ type: 'status', id: 'id-1' });
    void client.unlock('hunter2hunter2');
    expect(port.lastSent).toMatchObject({ type: 'unlock', secret: 'hunter2hunter2' });
    void client.lock();
    expect(port.lastSent).toMatchObject({ type: 'lock' });
    void client.erase();
    expect(port.lastSent).toMatchObject({ type: 'erase', confirm: true });
  });
});

describe('VaultClient id-correlation', () => {
  it('resolves the matching request by id and ignores unrelated ids', async () => {
    const port = new FakePort();
    const client = makeClient(port);
    const p1 = client.status();
    const p2 = client.unlock('passphrase-long');
    expect(port.sent.map((m) => m.id)).toEqual(['id-1', 'id-2']);

    // Deliver an unrelated id first — must not resolve anything.
    port.emit({ id: 'id-999', ok: true, state: 'unlocked' });
    // Resolve p2 (id-2) BEFORE p1 to prove correlation isn't FIFO.
    port.emit({ id: 'id-2', ok: true, state: 'unlocked' });
    const r2 = await p2;
    expect(r2).toEqual({ id: 'id-2', ok: true, state: 'unlocked' });

    port.emit({ id: 'id-1', ok: true, state: 'locked', hardware: 'tpm2', maxAttempts: 6, attemptsRemaining: 6 });
    const r1 = await p1;
    expect(r1).toMatchObject({ id: 'id-1', ok: true, state: 'locked', hardware: 'tpm2' });
  });

  it('surfaces a bad-secret error response with attemptsRemaining + delayMs', async () => {
    const port = new FakePort();
    const client = makeClient(port);
    const p = client.unlock('wrong-passphrase');
    port.emit({ id: 'id-1', ok: false, error: 'bad-secret', attemptsRemaining: 3, delayMs: 2000 });
    const r = await p;
    expect(r.ok).toBe(false);
    if (!r.ok) {
      expect(r.error).toBe('bad-secret');
      expect(r.attemptsRemaining).toBe(3);
      expect(r.delayMs).toBe(2000);
    }
  });

  it('drops a malformed inbound frame without resolving', async () => {
    const port = new FakePort();
    const client = makeClient(port);
    const p = client.status();
    port.emit('garbage');
    port.emit({ not: 'a response' });
    // Now a valid one.
    port.emit({ id: 'id-1', ok: true, state: 'locked', hardware: 'software', maxAttempts: 6, attemptsRemaining: 6 });
    const r = await p;
    expect(r.ok).toBe(true);
  });

  it('rejects in-flight requests on host disconnect (fail-safe)', async () => {
    const port = new FakePort();
    const client = makeClient(port);
    const p = client.status();
    port.drop();
    await expect(p).rejects.toThrow(/disconnected/);
    expect(client.connected).toBe(false);
  });

  it('fires onDisconnected handlers', () => {
    const port = new FakePort();
    const client = makeClient(port);
    const cb = jest.fn();
    client.onDisconnected(cb);
    client.connect();
    port.drop();
    expect(cb).toHaveBeenCalledTimes(1);
  });

  it('times out a request with no response', async () => {
    jest.useFakeTimers();
    const port = new FakePort();
    const client = new VaultClient({
      portFactory: () => port,
      idGenerator: () => 'id-1',
      requestTimeoutMs: 1000
    });
    const p = client.status();
    const assertion = expect(p).rejects.toThrow(/timed out/);
    jest.advanceTimersByTime(1001);
    await assertion;
    jest.useRealTimers();
  });
});

describe('evaluateSecretStrength (software-mode weak-secret rule)', () => {
  it('passes any secret under hardware enforcement', () => {
    expect(evaluateSecretStrength('1234', 'tpm2').ok).toBe(true);
    expect(evaluateSecretStrength('1234', 'secure-enclave').ok).toBe(true);
  });

  it('rejects all-digit secrets in software mode', () => {
    const r = evaluateSecretStrength('1234567890123', 'software'); // 13 digits, long enough
    expect(r.ok).toBe(false);
    expect(r.reason).toBe('all-digits');
  });

  it('rejects too-short secrets in software mode', () => {
    const r = evaluateSecretStrength('short-pass', 'software'); // 10 chars
    expect(r.ok).toBe(false);
    expect(r.reason).toBe('too-short');
  });

  it('accepts a strong passphrase in software mode', () => {
    const r = evaluateSecretStrength('correct horse battery staple', 'software');
    expect(r.ok).toBe(true);
  });

  it('enforces the documented minimum length boundary', () => {
    const justUnder = 'a'.repeat(MIN_SOFTWARE_SECRET_LENGTH - 1);
    const exact = 'aB' + 'a'.repeat(MIN_SOFTWARE_SECRET_LENGTH - 2); // length === min, has letters
    expect(evaluateSecretStrength(justUnder, 'software').ok).toBe(false);
    expect(evaluateSecretStrength(exact, 'software').ok).toBe(true);
  });
});
