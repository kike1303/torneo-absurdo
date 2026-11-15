import { describe, expect, it } from 'vitest';
import { NUMBER_HALO } from '../src/stage/effects.ts';
import { numberHalo } from '../src/three/overlay.ts';

// TA-36: a damage number reads over any fighter because it carries a halo in the background color
// of the arena, in both renderers.
describe('the halo of a damage number', () => {
  it('outlines the number all around in the background color (3D, DOM)', () => {
    const halo = numberHalo(0xf1f5f9);
    const parts = halo.split(', ');
    // Eight offsets and one soft blur, every one in the background color.
    expect(parts).toHaveLength(9);
    expect(parts.every((p) => p.endsWith('#f1f5f9'))).toBe(true);
    for (const corner of ['0.06em 0.06em', '-0.06em -0.06em', '0.06em -0.06em', '-0.06em 0.06em']) {
      expect(parts.some((p) => p.startsWith(corner))).toBe(true);
    }
    expect(numberHalo(0x020617)).toContain('#020617');
    expect(numberHalo(0)).toContain('#000000');
  });

  it('is thick enough to read and thin enough to keep the digits open (2D, Pixi)', () => {
    expect(NUMBER_HALO).toBeGreaterThanOrEqual(0.15);
    expect(NUMBER_HALO).toBeLessThanOrEqual(0.3);
  });
});
