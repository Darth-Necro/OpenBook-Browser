// SPDX-License-Identifier: MPL-2.0
// Unit tests: per-action confirmation gate; nothing runs without confirm.

import {
  performAction,
  type Action,
  type ActionRequest
} from '../actions.js';

function makeAction(run: jest.Mock): Action<{ url: string }> {
  return {
    kind: 'open-url',
    requiresConfirmation: true,
    run: run as unknown as Action<{ url: string }>['run']
  };
}

const request: ActionRequest = {
  kind: 'open-url',
  summary: 'Open https://example.com',
  payload: { url: 'https://example.com' }
};

describe('performAction (per-action confirmation)', () => {
  it('does NOT execute when the user declines', async () => {
    const run = jest.fn();
    const confirm = jest.fn().mockResolvedValue(false);
    const outcome = await performAction(makeAction(run), request, confirm);
    expect(confirm).toHaveBeenCalledWith(request);
    expect(run).not.toHaveBeenCalled();
    expect(outcome.status).toBe('declined');
  });

  it('executes only after an explicit positive confirmation', async () => {
    const run = jest.fn().mockResolvedValue({ ok: true, message: 'opened' });
    const confirm = jest.fn().mockResolvedValue(true);
    const outcome = await performAction(makeAction(run), request, confirm);
    expect(confirm).toHaveBeenCalledTimes(1);
    expect(run).toHaveBeenCalledWith({ url: 'https://example.com' });
    expect(outcome).toEqual({ status: 'executed', result: { ok: true, message: 'opened' } });
  });

  it('confirm is awaited BEFORE run is ever invoked (no auto-exec)', async () => {
    const order: string[] = [];
    const run = jest.fn().mockImplementation(async () => {
      order.push('run');
      return { ok: true, message: '' };
    });
    const confirm = jest.fn().mockImplementation(async () => {
      order.push('confirm');
      return true;
    });
    await performAction(makeAction(run), request, confirm);
    expect(order).toEqual(['confirm', 'run']);
  });

  it('surfaces an error if the action run throws (still gated)', async () => {
    const run = jest.fn().mockRejectedValue(new Error('boom'));
    const confirm = jest.fn().mockResolvedValue(true);
    const outcome = await performAction(makeAction(run), request, confirm);
    expect(outcome.status).toBe('error');
    if (outcome.status === 'error') expect(outcome.message).toBe('boom');
  });

  it('surfaces an error if the confirm callback throws and does not run', async () => {
    const run = jest.fn();
    const confirm = jest.fn().mockRejectedValue(new Error('confirm-failed'));
    const outcome = await performAction(makeAction(run), request, confirm);
    expect(run).not.toHaveBeenCalled();
    expect(outcome.status).toBe('error');
  });

  it('rejects an action that is not confirmation-gated (defensive)', async () => {
    const run = jest.fn();
    // Force an invalid action shape past the type system.
    const bad = { kind: 'x', requiresConfirmation: false, run } as unknown as Action;
    const confirm = jest.fn().mockResolvedValue(true);
    const outcome = await performAction(bad, request, confirm);
    expect(run).not.toHaveBeenCalled();
    expect(outcome.status).toBe('error');
  });
});
