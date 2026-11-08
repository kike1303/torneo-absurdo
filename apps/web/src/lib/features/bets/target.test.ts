import { describe, expect, it } from 'vitest';
import { board, summary } from './fixtures.js';
import { fightsToRead, targetFight } from './target.js';

// 012 RF42 and plan-c C1.
describe('the target fight', () => {
  it('is the current one while its window is open', () => {
    const current = summary(2);
    expect(targetFight(board({ current, next: summary(3) }))?.id).toBe(2);
  });

  it('is the next one while the current one is live', () => {
    const current = summary(2, { state: 'live' });
    expect(targetFight(board({ current, next: summary(3) }))?.id).toBe(3);
  });

  it('is none without an open window, or without a board', () => {
    const current = summary(2, { state: 'betting_closed' });
    expect(targetFight(board({ current, next: summary(3, { state: 'scheduled' }) }))).toBeNull();
    expect(targetFight(null)).toBeNull();
  });

  it('reads the target, the current and the last, once each', () => {
    const current = summary(2, { state: 'live' });
    const b = board({ current, next: summary(3), last: summary(1, { state: 'settled' }) });
    expect(fightsToRead(b)).toEqual([3, 2, 1]);
    expect(fightsToRead(board({ current: summary(2) }))).toEqual([2]);
    expect(fightsToRead(null)).toEqual([]);
  });
});
