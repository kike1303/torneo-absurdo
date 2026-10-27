import { describe, expect, it } from 'vitest';
import { STAGE_HEIGHT, STAGE_WIDTH, letterbox, logicalFontPx } from '../src/scene/index.ts';

// RF-A15: the 640×360 stage fits in the container without cropping.
describe('letterbox', () => {
  it.each([
    [375, 211],
    [1280, 720],
    [1280, 800],
    [320, 400],
  ])('%i×%i shows the whole stage, centered', (width, height) => {
    const box = letterbox({ width, height });
    expect(box.width).toBeLessThanOrEqual(width + 1e-9);
    expect(box.height).toBeLessThanOrEqual(height + 1e-9);
    expect(box.width / box.height).toBeCloseTo(STAGE_WIDTH / STAGE_HEIGHT, 9);
    expect(box.x).toBeCloseTo((width - box.width) / 2, 9);
    expect(box.y).toBeCloseTo((height - box.height) / 2, 9);
    // One side always touches the container.
    expect(Math.min(width - box.width, height - box.height)).toBeCloseTo(0, 9);
  });

  it('1280×720 is exactly 2×, and 375 wide is 375/640', () => {
    expect(letterbox({ width: 1280, height: 720 }).scale).toBe(2);
    expect(letterbox({ width: 375, height: 211 }).scale).toBeCloseTo(375 / 640, 9);
  });

  it('an empty container gives scale 0, and font sizes compensate the scale', () => {
    expect(letterbox({ width: 0, height: 0 }).scale).toBe(0);
    expect(logicalFontPx(14, 0.5)).toBe(28);
    expect(logicalFontPx(14, 0)).toBe(14);
  });
});
