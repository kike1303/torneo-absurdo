// From an AIR frame and its sprite to a manifest frame (002 RF-B5, plan-b decision 9).
import { fail } from './errors.ts';
import type { RgbaImage } from './image.ts';
import { INPUT_LIMITS } from './limits.ts';

export interface FrameImage extends RgbaImage {
  /** The feet, in frame coordinates (inside the frame: `0 ≤ pivot ≤ size`). */
  readonly pivotX: number;
  readonly pivotY: number;
  /** The pivot was raised to the lowest visible row (`FramePlacement.ground`). */
  readonly grounded?: true;
}

export interface FramePlacement {
  readonly axisX: number;
  readonly axisY: number;
  /** AIR offset. */
  readonly x: number;
  readonly y: number;
  readonly flipH: boolean;
  readonly flipV: boolean;
  /**
   * The frame must rest on the ground: when its visible pixels end above the pivot, the pivot
   * goes up to the lowest visible row. A MUGEN fall lifts the sprite above its axis because the
   * height comes from the physics, and the renderer has no physics (TA-24).
   */
  readonly ground?: boolean;
}

export const EMPTY_FRAME: FrameImage = Object.freeze({
  width: 1,
  height: 1,
  rgba: new Uint8Array(4),
  pivotX: 0,
  pivotY: 0,
});

export function mirror(image: RgbaImage, h: boolean, v: boolean): RgbaImage {
  if (!h && !v) return image;
  const { width, height, rgba } = image;
  const out = new Uint8Array(rgba.length);
  for (let y = 0; y < height; y++) {
    const sy = v ? height - 1 - y : y;
    for (let x = 0; x < width; x++) {
      const sx = h ? width - 1 - x : x;
      out.set(rgba.subarray((sy * width + sx) * 4, (sy * width + sx) * 4 + 4), (y * width + x) * 4);
    }
  }
  return { width, height, rgba: out };
}

function crop(image: RgbaImage, x0: number, y0: number, w: number, h: number): RgbaImage {
  const out = new Uint8Array(w * h * 4);
  for (let y = 0; y < h; y++) {
    const src = ((y0 + y) * image.width + x0) * 4;
    out.set(image.rgba.subarray(src, src + w * 4), y * w * 4);
  }
  return { width: w, height: h, rgba: out };
}

/** Bounding box of the visible pixels, or null when there are none. */
export function visibleBounds(
  image: RgbaImage,
): { x: number; y: number; w: number; h: number } | null {
  let minX = image.width;
  let minY = image.height;
  let maxX = -1;
  let maxY = -1;
  for (let y = 0; y < image.height; y++) {
    for (let x = 0; x < image.width; x++) {
      if ((image.rgba[(y * image.width + x) * 4 + 3] ?? 0) === 0) continue;
      if (x < minX) minX = x;
      if (x > maxX) maxX = x;
      if (y < minY) minY = y;
      if (y > maxY) maxY = y;
    }
  }
  return maxX < 0 ? null : { x: minX, y: minY, w: maxX - minX + 1, h: maxY - minY + 1 };
}

/**
 * The pivot is the sprite axis (mirrored with the sprite) minus the AIR offset; the transparent
 * borders are trimmed and the frame is padded until the pivot falls inside it (never below it
 * with `ground`).
 */
export function buildFrame(sprite: RgbaImage | null, at: FramePlacement): FrameImage {
  if (sprite === null) return EMPTY_FRAME;
  const flipped = mirror(sprite, at.flipH, at.flipV);
  let pivotX = (at.flipH ? sprite.width - at.axisX : at.axisX) - at.x;
  let pivotY = (at.flipV ? sprite.height - at.axisY : at.axisY) - at.y;
  const box = visibleBounds(flipped);
  if (box === null) return EMPTY_FRAME;
  const trimmed = crop(flipped, box.x, box.y, box.w, box.h);
  pivotX -= box.x;
  pivotY -= box.y;
  const left = Math.max(0, -pivotX);
  const top = Math.max(0, -pivotY);
  const right = Math.max(0, pivotX - trimmed.width);
  let bottom = Math.max(0, pivotY - trimmed.height);
  const grounded = at.ground === true && bottom > 0;
  if (grounded) {
    pivotY = trimmed.height;
    bottom = 0;
  }
  const extra = grounded ? { grounded: true as const } : {};
  const width = trimmed.width + left + right;
  const height = trimmed.height + top + bottom;
  const max = INPUT_LIMITS.spriteSideMax;
  if (width > max || height > max) fail('convert.pivot_too_far', { width, height });
  if (left === 0 && top === 0 && right === 0 && bottom === 0) {
    return { ...trimmed, pivotX, pivotY, ...extra };
  }
  const rgba = new Uint8Array(width * height * 4);
  for (let y = 0; y < trimmed.height; y++) {
    const src = y * trimmed.width * 4;
    rgba.set(trimmed.rgba.subarray(src, src + trimmed.width * 4), ((y + top) * width + left) * 4);
  }
  return { width, height, rgba, pivotX: pivotX + left, pivotY: pivotY + top, ...extra };
}
