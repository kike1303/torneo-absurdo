// Palettes (002 plan-b decision 8). Always 256 RGBA entries; index 0 is transparent (MUGEN).
import { fail } from './errors.ts';
import type { RgbaImage } from './image.ts';

export interface Palette {
  /** 256 × RGBA. */
  readonly rgba: Uint8Array;
  /** Colors actually declared (1–256); a pixel index beyond them is an error. */
  readonly colors: number;
}

/** SFF v1: exactly 768 bytes of RGB (the PCX trailer). */
export function paletteFromRgb(rgb: Uint8Array): Palette {
  if (rgb.length !== 768) fail('sff.bad_palette', { length: rgb.length });
  const rgba = new Uint8Array(1024);
  for (let i = 0; i < 256; i++) {
    rgba[i * 4] = rgb[i * 3] ?? 0;
    rgba[i * 4 + 1] = rgb[i * 3 + 1] ?? 0;
    rgba[i * 4 + 2] = rgb[i * 3 + 2] ?? 0;
    rgba[i * 4 + 3] = i === 0 ? 0 : 255;
  }
  return { rgba, colors: 256 };
}

/** SFF v2: `numcols` × 4 bytes (RGB plus a reserved byte), padded with transparent black. */
export function paletteFromV2(data: Uint8Array, numcols: number): Palette {
  if (!Number.isInteger(numcols) || numcols < 1 || numcols > 256) {
    fail('sff.bad_palette', { colors: numcols });
  }
  if (data.length !== numcols * 4) fail('sff.bad_palette', { length: data.length });
  const rgba = new Uint8Array(1024);
  for (let i = 0; i < numcols; i++) {
    rgba[i * 4] = data[i * 4] ?? 0;
    rgba[i * 4 + 1] = data[i * 4 + 1] ?? 0;
    rgba[i * 4 + 2] = data[i * 4 + 2] ?? 0;
    rgba[i * 4 + 3] = i === 0 ? 0 : 255;
  }
  return { rgba, colors: numcols };
}

/** Indexed pixels (`stride` bytes per row, `width` used) to RGBA. */
export function applyPalette(
  indices: Uint8Array,
  width: number,
  height: number,
  stride: number,
  palette: Palette,
): RgbaImage {
  const rgba = new Uint8Array(width * height * 4);
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const index = indices[y * stride + x] ?? 0;
      if (index >= palette.colors) fail('sff.palette_index_out_of_range', { index });
      const o = (y * width + x) * 4;
      rgba[o] = palette.rgba[index * 4] ?? 0;
      rgba[o + 1] = palette.rgba[index * 4 + 1] ?? 0;
      rgba[o + 2] = palette.rgba[index * 4 + 2] ?? 0;
      rgba[o + 3] = palette.rgba[index * 4 + 3] ?? 0;
    }
  }
  return { width, height, rgba };
}
