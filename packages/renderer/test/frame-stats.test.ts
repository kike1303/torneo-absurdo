import { describe, expect, it } from 'vitest';
import { EMPTY_STATS, createFrameRecorder, percentile } from '../src/stats/frame-stats.ts';

// RF-A19 and plan-a2 decision 13.
describe('frame stats', () => {
  it('nearest-rank percentiles of a known series', () => {
    const sorted = Array.from({ length: 100 }, (_, i) => i + 1);
    expect(percentile(sorted, 50)).toBe(50);
    expect(percentile(sorted, 95)).toBe(95);
    expect(percentile(sorted, 99)).toBe(99);
    expect(percentile([7], 99)).toBe(7);
    expect(percentile([], 50)).toBe(0);
  });

  it('frames, mean FPS and frames over 33 ms', () => {
    const r = createFrameRecorder();
    for (let i = 0; i < 90; i++) r.record(16);
    for (let i = 0; i < 10; i++) r.record(40);
    const s = r.stats();
    expect(s.frames).toBe(100);
    expect(s.over33).toBe(10);
    expect(s.fpsMean).toBeCloseTo(100_000 / (90 * 16 + 400), 6);
    expect(s.p50).toBe(16);
    expect(s.p95).toBe(40);
  });

  it('keeps the last intervals when the ring overflows', () => {
    const r = createFrameRecorder(4);
    for (const ms of [50, 50, 50, 10, 10, 10, 10]) r.record(ms);
    expect(r.stats()).toMatchObject({ frames: 4, p99: 10, over33: 0 });
  });

  it('reset empties it, and invalid intervals are ignored', () => {
    const r = createFrameRecorder();
    r.record(16);
    r.record(Number.NaN);
    r.record(0);
    expect(r.stats().frames).toBe(1);
    r.reset();
    expect(r.stats()).toEqual(EMPTY_STATS);
  });
});
