import { describe, expect, it } from 'vitest';
import type { FightEvent } from '@torneo/protocol';
import { formatClock, roundClockMs } from './round-clock.js';

// 010 RF55 and plan-d decisions 12 and 14.
const LIMIT = 86_000;
const KO: FightEvent = { type: 'ko', t: 50_000, side: 'a' };
const TIME_UP: FightEvent = { type: 'time_up', t: LIMIT };

describe('roundClockMs', () => {
  it('counts down by t', () => {
    expect(roundClockMs(LIMIT, [], 0)).toBe(LIMIT);
    expect(roundClockMs(LIMIT, [], 1000)).toBe(LIMIT - 1000);
  });

  it('before the start it shows the whole round', () => {
    expect(roundClockMs(LIMIT, [], -3000)).toBe(LIMIT);
  });

  it('a ko already received does not stop it before its moment', () => {
    expect(roundClockMs(LIMIT, [KO], 49_999)).toBe(LIMIT - 49_999);
  });

  it('stops at the ko, also long after', () => {
    expect(roundClockMs(LIMIT, [KO], 50_000)).toBe(LIMIT - 50_000);
    expect(roundClockMs(LIMIT, [KO], 54_000)).toBe(LIMIT - 50_000);
  });

  it('reaches zero at time_up and stays there', () => {
    expect(roundClockMs(LIMIT, [TIME_UP], LIMIT)).toBe(0);
    expect(roundClockMs(LIMIT, [TIME_UP], 90_000)).toBe(0);
  });

  it('never goes below zero without an event', () => {
    expect(roundClockMs(LIMIT, [], LIMIT + 5000)).toBe(0);
  });
});

describe('formatClock', () => {
  it.each([
    [0, '0:00'],
    [1, '0:01'],
    [59_000, '0:59'],
    [60_000, '1:00'],
    [599_500, '10:00'],
    [3_600_000, '1:00:00'],
    [3_661_000, '1:01:01'],
    [-5, '0:00'],
  ])('%i ms is %s', (ms, text) => {
    expect(formatClock(ms, 'es')).toBe(text);
  });
});
