import { describe, expect, it } from 'vitest';
import { buildFrame, EMPTY_FRAME, mirror } from '../src/frames.ts';
import type { RgbaImage } from '../src/image.ts';
import { code } from './support/expect-error.ts';

/** A w × h image whose pixel (x, y) has red = 10·y + x + 1, alpha 255, except listed holes. */
function image(
  w: number,
  h: number,
  transparent: (x: number, y: number) => boolean = () => false,
): RgbaImage {
  const rgba = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      if (!transparent(x, y)) rgba.set([10 * y + x + 1, 0, 0, 255], (y * w + x) * 4);
    }
  }
  return { width: w, height: h, rgba };
}

const red = (img: RgbaImage) =>
  Array.from({ length: img.width * img.height }, (_, i) => img.rgba[i * 4] ?? 0);
const place = { axisX: 2, axisY: 3, x: 0, y: 0, flipH: false, flipV: false };

describe('buildFrame (RF-B5)', () => {
  it('the pivot is the axis minus the AIR offset', () => {
    const f = buildFrame(image(4, 4), { ...place, x: 1, y: -1 });
    expect([f.width, f.height, f.pivotX, f.pivotY]).toEqual([4, 4, 1, 4]);
  });

  it('mirrors H and V, pixels and pivot', () => {
    expect(red(mirror(image(3, 1), true, false))).toEqual([3, 2, 1]);
    expect(red(mirror(image(1, 2), false, true))).toEqual([11, 1]);
    const f = buildFrame(image(4, 4), { ...place, flipH: true });
    expect(f.pivotX).toBe(2);
    expect(red(f).slice(0, 4)).toEqual([4, 3, 2, 1]);
    expect(buildFrame(image(4, 6), { ...place, flipV: true }).pivotY).toBe(3);
  });

  it('trims the transparent borders and moves the pivot with them', () => {
    const f = buildFrame(
      image(5, 5, (x, y) => x === 0 || y === 4),
      place,
    );
    expect([f.width, f.height, f.pivotX, f.pivotY]).toEqual([4, 4, 1, 3]);
    expect(red(f)[0]).toBe(2);
  });

  it('pads with transparent pixels until the pivot is inside', () => {
    const f = buildFrame(image(2, 2), { ...place, axisX: -3, axisY: 9 });
    expect([f.width, f.height, f.pivotX, f.pivotY]).toEqual([5, 9, 0, 9]);
    expect(f.rgba[3]).toBe(0); // padded column
    expect(red(f)[3]).toBe(1); // the sprite starts at x = 3
  });

  it('a pivot too far is convert.pivot_too_far', () => {
    expect(code(() => buildFrame(image(2, 2), { ...place, axisX: 2000 }))).toBe(
      'convert.pivot_too_far',
    );
  });

  it('no sprite or no visible pixels is the 1 × 1 empty frame', () => {
    expect(buildFrame(null, place)).toBe(EMPTY_FRAME);
    expect(
      buildFrame(
        image(3, 3, () => true),
        place,
      ),
    ).toBe(EMPTY_FRAME);
    expect([EMPTY_FRAME.width, EMPTY_FRAME.height, EMPTY_FRAME.pivotX, EMPTY_FRAME.pivotY]).toEqual(
      [1, 1, 0, 0],
    );
  });
});
