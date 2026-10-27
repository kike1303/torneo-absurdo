import { describe, expect, it } from 'vitest';
import { INITIAL_PLAYBACK, pause, play, seek, timeAt } from '../src/player/playback.ts';

// plan-a2 decision 2: t is a pure function of the anchor and the clock.
const D = 10_000;

describe('playback clock', () => {
  it('stays at 0 until play, then follows the clock (identity anchor)', () => {
    expect(timeAt(INITIAL_PLAYBACK, 500, D)).toBe(0);
    const p = play(INITIAL_PLAYBACK, 500, D);
    expect(timeAt(p, 500, D)).toBe(500);
    expect(timeAt(p, 1500, D)).toBe(1500);
  });

  it('pause freezes and play resumes from the same t', () => {
    let p = seek(INITIAL_PLAYBACK, 100, 0, D);
    p = play(p, 100, D);
    p = pause(p, 1100, D);
    expect(timeAt(p, 5000, D)).toBe(1000);
    p = play(p, 5000, D);
    expect(timeAt(p, 5500, D)).toBe(1500);
  });

  it('seek keeps playing or paused', () => {
    const playing = seek(play(seek(INITIAL_PLAYBACK, 0, 0, D), 0, D), 200, 4000, D);
    expect(timeAt(playing, 300, D)).toBe(4100);
    const paused = seek(INITIAL_PLAYBACK, 200, 4000, D);
    expect(timeAt(paused, 900, D)).toBe(4000);
  });

  it('clamps t to [0, duration]', () => {
    const p = play(seek(INITIAL_PLAYBACK, 0, 0, D), 0, D);
    expect(timeAt(p, D + 5000, D)).toBe(D);
    expect(timeAt(p, -50, D)).toBe(0);
    expect(timeAt(seek(INITIAL_PLAYBACK, 0, Number.NaN, D), 0, D)).toBe(0);
    expect(timeAt(seek(INITIAL_PLAYBACK, 0, -3, D), 0, D)).toBe(0);
  });

  it('play and pause are idempotent', () => {
    const p = play(seek(INITIAL_PLAYBACK, 0, 0, D), 0, D);
    expect(play(p, 900, D)).toBe(p);
    const q = pause(p, 900, D);
    expect(pause(q, 2000, D)).toBe(q);
  });
});
