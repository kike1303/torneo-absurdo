// Color math of the wardrobe (022), in sRGB 0..1. No Three here.

export type Rgb = readonly [number, number, number];

const HEX = /^#[0-9a-f]{6}$/i;

/** `#RRGGBB` only; anything else is not a color. */
export function parseHex(value: unknown): Rgb | null {
  if (typeof value !== 'string' || !HEX.test(value)) return null;
  const channel = (at: number): number => parseInt(value.slice(at, at + 2), 16) / 255;
  return [channel(1), channel(3), channel(5)];
}

export function toHsv(r: number, g: number, b: number): [number, number, number] {
  const hi = Math.max(r, g, b);
  const d = hi - Math.min(r, g, b);
  let h = 0;
  if (d > 1e-6) {
    if (hi === r) h = (((g - b) / d + 6) % 6) / 6;
    else if (hi === g) h = ((b - r) / d + 2) / 6;
    else h = ((r - g) / d + 4) / 6;
  }
  return [h, hi > 1e-6 ? d / hi : 0, hi];
}

export function fromHsv(h: number, s: number, v: number): [number, number, number] {
  const f = (n: number): number => {
    const k = (n + h * 6) % 6;
    return v - v * s * Math.max(0, Math.min(k, 4 - k, 1));
  };
  return [f(5), f(3), f(1)];
}
