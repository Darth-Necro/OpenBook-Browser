// SPDX-License-Identifier: MPL-2.0
// Unit tests for pure lock-screen presentation helpers.

import {
  attemptWarning,
  formatDelay,
  hardwareLabel,
  showsSoftwareFallbackWarning,
  stateLabel,
  ERASE_WARNING_THRESHOLD
} from '../lockstate';

describe('attemptWarning', () => {
  it('is critical and final at 0', () => {
    const w = attemptWarning(0);
    expect(w.level).toBe('critical');
    expect(w.message).toMatch(/erased/i);
  });
  it('is critical at 1 and warns the next failure erases', () => {
    const w = attemptWarning(1);
    expect(w.level).toBe('critical');
    expect(w.message).toMatch(/NEXT/);
  });
  it('warns at the threshold', () => {
    const w = attemptWarning(ERASE_WARNING_THRESHOLD);
    expect(w.level).toBe('warn');
  });
  it('is none well above the threshold', () => {
    expect(attemptWarning(5).level).toBe('none');
  });
});

describe('formatDelay', () => {
  it('formats sub-minute', () => {
    expect(formatDelay(0)).toBe('0s');
    expect(formatDelay(900)).toBe('1s');
    expect(formatDelay(5000)).toBe('5s');
  });
  it('formats minutes', () => {
    expect(formatDelay(60000)).toBe('1m');
    expect(formatDelay(90000)).toBe('1m 30s');
  });
});

describe('hardwareLabel + fallback warning', () => {
  it('labels each backing', () => {
    expect(hardwareLabel('tpm2')).toMatch(/TPM/);
    expect(hardwareLabel('secure-enclave')).toMatch(/Secure Enclave/);
    expect(hardwareLabel('software')).toMatch(/weaker/i);
  });
  it('shows the fallback warning only in software mode', () => {
    expect(showsSoftwareFallbackWarning('software')).toBe(true);
    expect(showsSoftwareFallbackWarning('tpm2')).toBe(false);
    expect(showsSoftwareFallbackWarning('secure-enclave')).toBe(false);
  });
});

describe('stateLabel', () => {
  it('labels each state', () => {
    expect(stateLabel('uninitialized')).toMatch(/set up/i);
    expect(stateLabel('locked')).toBe('Locked');
    expect(stateLabel('unlocked')).toBe('Unlocked');
    expect(stateLabel('erased')).toMatch(/unrecoverable/i);
  });
});
