import { describe, expect, it } from 'vitest';
import {
  FRAME_GAPS_MS,
  MIN_PIXEL_RATIO,
  SOFTWARE_FRAME_MS,
  createPacer,
} from '../src/three/pacer.ts';

/** `count` frames `gap` ms apart; returns how many times the pixel ratio changed. */
function frames(pacer: ReturnType<typeof createPacer>, gap: number, count: number, hidden = false) {
  let changes = 0;
  for (let i = 0; i < count; i++) if (pacer.observe(gap, hidden)) changes++;
  return changes;
}

describe('createPacer', () => {
  it('draws every frame at full ratio on a device that keeps up', () => {
    const pacer = createPacer({ pixelRatio: 2, software: false });
    expect(frames(pacer, 16.7, 600)).toBe(0);
    expect(pacer.pixelRatio).toBe(2);
    expect(pacer.minFrameMs).toBe(0);
    for (let t = 0; t < 1000; t += 16.7) expect(pacer.draw(t)).toBe(true);
  });

  it('starts software WebGL at the minimum ratio and 100 ms between drawn frames', () => {
    const pacer = createPacer({ pixelRatio: 3, software: true });
    expect(pacer.pixelRatio).toBe(MIN_PIXEL_RATIO);
    expect(pacer.minFrameMs).toBe(SOFTWARE_FRAME_MS);
    const drawn = Array.from({ length: 60 }, (_, i) => pacer.draw(i * 16.7)).filter(Boolean);
    expect(drawn.length).toBeLessThanOrEqual(11);
  });

  it('first draws fewer pixels, then fewer frames, when frames stay slow', () => {
    const pacer = createPacer({ pixelRatio: 2, software: false });
    let ratioSteps = 0;
    for (let i = 0; i < 400 && pacer.minFrameMs < FRAME_GAPS_MS.at(-1)!; i++)
      ratioSteps += frames(pacer, 80, 1);
    expect(pacer.pixelRatio).toBe(MIN_PIXEL_RATIO);
    expect(ratioSteps).toBeGreaterThan(0);
    expect(pacer.minFrameMs).toBe(FRAME_GAPS_MS.at(-1));
  });

  it('counts stalls in a row, the frames of a starved GPU (CI of #93)', () => {
    const pacer = createPacer({ pixelRatio: 1, software: false });
    frames(pacer, 4000, 3);
    expect(pacer.pixelRatio).toBeLessThan(1);
  });

  it('never steps down for a lone stall, as when a tab is shown again (review of #93)', () => {
    const pacer = createPacer({ pixelRatio: 1, software: false });
    for (let i = 0; i < 10; i++) {
      // The first frame after the tab comes back carries all the time away, already visible.
      frames(pacer, 30_000, 1);
      frames(pacer, 16.7, 120);
    }
    expect(pacer.pixelRatio).toBe(1);
    expect(pacer.minFrameMs).toBe(0);
  });

  it('ignores the gaps of a hidden page', () => {
    const pacer = createPacer({ pixelRatio: 2, software: false });
    expect(frames(pacer, 5000, 20, true)).toBe(0);
    expect(pacer.pixelRatio).toBe(2);
    expect(pacer.minFrameMs).toBe(0);
  });

  it('steps at most once per cooldown', () => {
    const pacer = createPacer({ pixelRatio: 2, software: false });
    // The first stall is lone (ignored); from the second on they count, one step per cooldown.
    expect(frames(pacer, 1000, 2)).toBe(0);
    expect(frames(pacer, 1000, 1)).toBe(1);
    expect(frames(pacer, 100, 3)).toBe(0);
  });
});
