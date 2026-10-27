// SFF entry points (002 RF-B3): version detection, lookup, links and lazy decoding.
import { fail } from './errors.ts';
import type { RgbaImage } from './image.ts';
import { INPUT_LIMITS } from './limits.ts';
import { decodeLz5 } from './lz5.ts';
import { applyPalette, paletteFromRgb, paletteFromV2, type Palette } from './palette.ts';
import {
  checkPcxHeader,
  decodePcx,
  isSupportedPcx,
  PCX_HEADER,
  pcxPalette,
  readPcxHeader,
} from './pcx.ts';
import { decodePng, type PngFormat } from './png.ts';
import { ByteReader } from './reader.ts';
import { decodeRle8 } from './rle8.ts';
import { readSffV1 } from './sff-v1.ts';
import { readSffV2 } from './sff-v2.ts';
import type { Compression, SffFile, SffV1, SffV2, SpriteNodeV2 } from './sff-types.ts';

export type * from './sff-types.ts';

const SIGNATURE = 'ElecbyteSpr\0';
const V2_FORMATS: Readonly<Record<number, Compression>> = {
  0: 'raw',
  2: 'rle8',
  3: 'rle5',
  4: 'lz5',
  10: 'png8',
  11: 'png24',
  12: 'png32',
};

/** Headers and nodes only; no pixel is decoded here. */
export function readSff(bytes: Uint8Array): SffFile {
  if (bytes.length > INPUT_LIMITS.sffBytes) fail('sff.too_large', { length: bytes.length });
  const r = new ByteReader(bytes);
  const sig = r.bytesView(12);
  for (let i = 0; i < SIGNATURE.length; i++) {
    if (sig[i] !== SIGNATURE.charCodeAt(i)) fail('sff.bad_signature');
  }
  const verlo3 = r.u8();
  const verlo2 = r.u8();
  const verlo1 = r.u8();
  const verhi = r.u8();
  void verlo3;
  const label = `${verhi}.${verlo1}${verlo2}`;
  if (verhi === 1) return readSffV1(r, label);
  if (verhi === 2) return readSffV2(r, label);
  return fail('sff.unsupported_version', { version: label });
}

/** First sprite with that group and image, or -1. */
export function spriteIndexer(file: SffFile): (group: number, image: number) => number {
  const byKey = new Map<string, number>();
  for (const s of file.sprites) {
    const key = `${s.group},${s.image}`;
    if (!byKey.has(key)) byKey.set(key, s.index);
  }
  return (group, image) => byKey.get(`${group},${image}`) ?? -1;
}

/** Follows links at most `count` steps (no recursion over input). */
function followLinks(
  start: number,
  count: number,
  linkOf: (index: number) => number | null,
  what: 'sprite' | 'palette',
): number {
  let index = start;
  for (let step = 0; step <= count; step++) {
    const next = linkOf(index);
    if (next === null) return index;
    if (next >= count) fail('sff.link_out_of_range', { [what]: index, linked: next });
    index = next;
  }
  return fail('sff.link_cycle', { [what]: start });
}

export function resolveSprite(file: SffFile, index: number): number {
  const sprites = file.sprites;
  if (!Number.isInteger(index) || index < 0 || index >= sprites.length) {
    fail('sff.link_out_of_range', { sprite: index });
  }
  return followLinks(index, sprites.length, (i) => sprites[i]?.linked ?? null, 'sprite');
}

export interface SpriteInfo {
  readonly width: number;
  readonly height: number;
  readonly compression: Compression;
  readonly supported: boolean;
}

/** Size and compression of the data a sprite uses, from headers only. */
export function spriteInfo(file: SffFile, index: number): SpriteInfo {
  const owner = resolveSprite(file, index);
  if (file.version === 1) {
    const data = file.sprites[owner]?.data;
    if (data == null) return fail('sff.link_cycle', { sprite: index });
    const h = readPcxHeader(data.bytes);
    return { width: h.width, height: h.height, compression: 'pcx', supported: isSupportedPcx(h) };
  }
  const node = file.sprites[owner] as SpriteNodeV2;
  const compression = V2_FORMATS[node.format] ?? 'unknown';
  const supported = compression !== 'rle5' && compression !== 'unknown';
  return { width: node.width, height: node.height, compression, supported };
}

/**
 * The compression of a sprite that owns its data and cannot be decoded, or null. Never throws:
 * it reads only node fields and, in SFF v1, a PCX header that is fully present. A sprite that
 * nobody maps is only reported (RF-B3); broken data surfaces when a mapped one is decoded.
 */
export function unsupportedCompression(file: SffFile, index: number): Compression | null {
  if (file.version === 1) {
    const data = file.sprites[index]?.data;
    if (data == null || data.length < PCX_HEADER) return null;
    return isSupportedPcx(readPcxHeader(data.bytes)) ? null : 'pcx';
  }
  const node = file.sprites[index];
  if (node === undefined || node.data === null) return null;
  const compression = V2_FORMATS[node.format] ?? 'unknown';
  return compression === 'rle5' || compression === 'unknown' ? compression : null;
}

export interface DecodeOptions {
  /** SFF v1 with a shared palette: the sprite whose palette the "same palette" sprites use. */
  readonly paletteSprite?: number;
}

function ownPaletteV1(file: SffV1, index: number): Palette | null {
  const data = file.sprites[resolveSprite(file, index)]?.data;
  if (data == null) return null;
  const rgb = pcxPalette(data.bytes);
  return rgb === null ? null : paletteFromRgb(rgb);
}

/** Plan-b decision 8, SFF v1. Iterative: walks back at most `index` sprites. */
function paletteV1(file: SffV1, index: number, options: DecodeOptions): Palette {
  for (let i = index; i >= 0; i--) {
    const sprite = file.sprites[i];
    if (sprite === undefined) break;
    if (file.sharedPalette && sprite.samePalette) {
      const source = options.paletteSprite ?? -1;
      const shared = source >= 0 ? ownPaletteV1(file, source) : null;
      if (shared === null) fail('sff.missing_palette', { sprite: index });
      return shared;
    }
    if (!sprite.samePalette) {
      const own = ownPaletteV1(file, i);
      if (own !== null) return own;
    }
  }
  return fail('sff.missing_palette', { sprite: index });
}

function paletteV2(file: SffV2, paletteIndex: number): Palette {
  const palettes = file.palettes;
  if (paletteIndex >= palettes.length) {
    fail('sff.palette_not_found', { palette: paletteIndex });
  }
  const owner = followLinks(
    paletteIndex,
    palettes.length,
    (i) => palettes[i]?.linked ?? null,
    'palette',
  );
  const node = palettes[owner];
  if (node?.data == null) return fail('sff.palette_not_found', { palette: paletteIndex });
  // The color count comes from the node that owns the data.
  return paletteFromV2(node.data.bytes, node.colors);
}

function checkSize(width: number, height: number): void {
  const max = INPUT_LIMITS.spriteSideMax;
  if (width < 1 || height < 1 || width > max || height > max) {
    fail('sff.bad_sprite_size', { width, height });
  }
}

function compressed(data: ByteReader, size: number): Uint8Array {
  const r = new ByteReader(data.bytes);
  const declared = r.u32();
  if (declared !== size) fail('sff.size_mismatch', { declared, expected: size });
  return r.bytesView(r.remaining);
}

function decodeV2(file: SffV2, index: number): RgbaImage {
  const owner = resolveSprite(file, index);
  const node = file.sprites[owner] as SpriteNodeV2;
  if (node.data === null) return fail('sff.link_cycle', { sprite: index });
  const { width, height } = node;
  checkSize(width, height);
  const size = width * height;
  const compression = V2_FORMATS[node.format] ?? 'unknown';
  switch (compression) {
    case 'raw': {
      if (node.data.length < size)
        fail('sff.size_mismatch', { declared: node.data.length, expected: size });
      return applyPalette(
        node.data.bytes,
        width,
        height,
        width,
        paletteV2(file, node.paletteIndex),
      );
    }
    case 'rle8':
      return applyPalette(
        decodeRle8(compressed(node.data, size), size),
        width,
        height,
        width,
        paletteV2(file, node.paletteIndex),
      );
    case 'lz5':
      return applyPalette(
        decodeLz5(compressed(node.data, size), size),
        width,
        height,
        width,
        paletteV2(file, node.paletteIndex),
      );
    case 'png8':
    case 'png24':
    case 'png32': {
      // The u32 before a PNG is not the pixel count, so it is skipped, not compared.
      const r = new ByteReader(node.data.bytes);
      r.u32();
      const png = decodePng(r.bytesView(r.remaining), node.format as PngFormat, width, height);
      if (png.bpp === 1) {
        return applyPalette(png.pixels, width, height, width, paletteV2(file, node.paletteIndex));
      }
      if (png.bpp === 4) return { width, height, rgba: png.pixels };
      const rgba = new Uint8Array(size * 4);
      for (let i = 0; i < size; i++) {
        rgba[i * 4] = png.pixels[i * 3] ?? 0;
        rgba[i * 4 + 1] = png.pixels[i * 3 + 1] ?? 0;
        rgba[i * 4 + 2] = png.pixels[i * 3 + 2] ?? 0;
        rgba[i * 4 + 3] = 255;
      }
      return { width, height, rgba };
    }
    default:
      return fail('sff.unsupported_compression', { sprite: index, format: node.format });
  }
}

function decodeV1(file: SffV1, index: number, options: DecodeOptions): RgbaImage {
  const owner = resolveSprite(file, index);
  const data = file.sprites[owner]?.data;
  if (data == null) return fail('sff.link_cycle', { sprite: index });
  checkPcxHeader(readPcxHeader(data.bytes));
  const { header, indices } = decodePcx(data.bytes);
  return applyPalette(
    indices,
    header.width,
    header.height,
    header.bytesPerLine,
    paletteV1(file, index, options),
  );
}

/** Decodes one sprite to RGBA (plan-b decision 9: only the mapped ones, one at a time). */
export function decodeSprite(file: SffFile, index: number, options: DecodeOptions = {}): RgbaImage {
  return file.version === 1 ? decodeV1(file, index, options) : decodeV2(file, index);
}
