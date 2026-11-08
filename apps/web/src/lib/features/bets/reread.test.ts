import { describe, expect, it, vi } from 'vitest';
import { MAX_REREAD_DELAY_MS, createReread } from './reread.js';

// 012 RF36, plan-c C5.
describe('the reread of the payout', () => {
  it('reads again once per fight, after a random wait under 3 s, and only with a bet', () => {
    const timers: number[] = [];
    const reread = vi.fn();
    const r = createReread({
      random: () => 0.999,
      setTimer: (run, ms) => {
        timers.push(ms);
        run();
      },
      reread,
    });
    r.due(1, false);
    expect(reread).not.toHaveBeenCalled();
    r.due(2, true);
    r.due(2, true);
    expect(reread).toHaveBeenCalledTimes(1);
    expect(timers[0]).toBeLessThan(MAX_REREAD_DELAY_MS);
    expect(timers[0]).toBeGreaterThanOrEqual(0);
    r.due(3, true);
    expect(reread).toHaveBeenCalledTimes(2);
  });
});
