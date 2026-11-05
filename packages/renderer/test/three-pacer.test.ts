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

  it('starts software WebGL at the minimum ratio and about 10 frames per second', () => {
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

  it('counts seconds-long frames, the stall of a starved GPU (review of #93)', () => {
    const pacer = createPacer({ pixelRatio: 1, software: false });
    frames(pacer, 4000, 2);
    expect(pacer.pixelRatio).toBeLessThan(1);
  });

  it('ignores the gaps of a hidden page', () => {
    const pacer = createPacer({ pixelRatio: 2, software: false });
    expect(frames(pacer, 5000, 20, true)).toBe(0);
    expect(pacer.pixelRatio).toBe(2);
    expect(pacer.minFrameMs).toBe(0);
  });

  it('steps at most once per cooldown', () => {
    const pacer = createPacer({ pixelRatio: 2, software: false });
    expect(frames(pacer, 1000, 1)).toBe(0);
    expect(frames(pacer, 1000, 1)).toBe(1);
    expect(frames(pacer, 100, 3)).toBe(0);
  });
});
