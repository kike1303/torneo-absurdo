import { describe, expect, it } from 'vitest';
import { cursorAt, dueCues, lastCueAt } from '../src/player/cues.ts';

// plan-a2 decision 3: the cues with cursor ≤ index and at_ms ≤ t.
const TIMES = [0, 2000, 2600, 2600, 4000];

describe('cue dispatch', () => {
  it('returns the cues up to t from the cursor', () => {
    expect(dueCues(TIMES, 0, 0)).toEqual({ from: 0, to: 1 });
    expect(dueCues(TIMES, 1, 1999)).toEqual({ from: 1, to: 1 });
    expect(dueCues(TIMES, 1, 2600)).toEqual({ from: 1, to: 4 });
    expect(dueCues(TIMES, 4, 10_000)).toEqual({ from: 4, to: 5 });
  });

  it('never moves the cursor back when the clock does', () => {
    expect(dueCues(TIMES, 4, 100)).toEqual({ from: 4, to: 4 });
  });

  it('a seek puts the cursor on the first cue at or after t', () => {
    expect(cursorAt(TIMES, 2600)).toBe(2);
    expect(cursorAt(TIMES, 2601)).toBe(4);
    expect(cursorAt(TIMES, 0)).toBe(0);
  });

  it('lastCueAt', () => {
    expect(lastCueAt(TIMES, -1)).toBe(-1);
    expect(lastCueAt(TIMES, 2600)).toBe(3);
    expect(lastCueAt(TIMES, 99_999)).toBe(4);
  });
});
