// A two-color pattern texture (022, `extras.tint_pattern`): it is drawn with the default colors of
// two slots and blends of them. Each texel keeps its place between the two and moves to the pair
// the fighter wears.
import type { Rgb } from './color.ts';

/** Colors in sRGB 0..1; `src` and `dst` are RGBA bytes. */
export function remapPattern(
  src: Uint8ClampedArray,
  dst: Uint8ClampedArray,
  from: readonly [Rgb, Rgb],
  to: readonly [Rgb, Rgb],
): void {
  const [a0, b0] = from;
  const [a1, b1] = to;
  const axis = [b0[0] - a0[0], b0[1] - a0[1], b0[2] - a0[2]] as const;
  const length2 = axis[0] ** 2 + axis[1] ** 2 + axis[2] ** 2 || 1;
  for (let i = 0; i < src.length; i += 4) {
    const r = (src[i] ?? 0) / 255 - a0[0];
    const g = (src[i + 1] ?? 0) / 255 - a0[1];
    const b = (src[i + 2] ?? 0) / 255 - a0[2];
    const t = Math.min(1, Math.max(0, (r * axis[0] + g * axis[1] + b * axis[2]) / length2));
    dst[i] = (a1[0] + (b1[0] - a1[0]) * t) * 255;
    dst[i + 1] = (a1[1] + (b1[1] - a1[1]) * t) * 255;
    dst[i + 2] = (a1[2] + (b1[2] - a1[2]) * t) * 255;
    dst[i + 3] = src[i + 3] ?? 255;
  }
}
