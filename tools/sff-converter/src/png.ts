// PNG of SFF v2 (formats 10, 11, 12), decoded here (plan-b decision 7): pngjs would expand the
// palette and lose the indices. The CRC is ignored; the inflated size is bounded and exact.
import { inflateSync } from 'node:zlib';
import { fail } from './errors.ts';
import { ByteReader } from './reader.ts';

const SIGNATURE = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
/** Color type and bytes per pixel by SFF v2 format. */
const KINDS = {
  10: { colorType: 3, bpp: 1 },
  11: { colorType: 2, bpp: 3 },
  12: { colorType: 6, bpp: 4 },
} as const;
export type PngFormat = keyof typeof KINDS;

export interface PngPixels {
  readonly width: number;
  readonly height: number;
  /** Bytes per pixel: 1 (indices), 3 (RGB) or 4 (RGBA). */
  readonly bpp: number;
  readonly pixels: Uint8Array;
}

function chunkType(r: ByteReader): string {
  const t = r.bytesView(4);
  return String.fromCharCode(t[0] ?? 0, t[1] ?? 0, t[2] ?? 0, t[3] ?? 0);
}

function inflate(idat: Uint8Array, expected: number): Uint8Array {
  let raw: Uint8Array;
  try {
    raw = inflateSync(idat, { maxOutputLength: Math.max(1, expected) });
  } catch (error) {
    const code = (error as { code?: unknown } | null)?.code;
    fail(code === 'ERR_BUFFER_TOO_LARGE' ? 'png.too_large' : 'png.corrupt');
  }
  if (raw.length !== expected) fail('png.size_mismatch', { length: raw.length, expected });
  return raw;
}

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  return pb <= pc ? b : c;
}

function unfilter(raw: Uint8Array, width: number, height: number, bpp: number): Uint8Array {
  const stride = width * bpp;
  const out = new Uint8Array(stride * height);
  for (let y = 0; y < height; y++) {
    const filter = raw[y * (stride + 1)] ?? 0;
    const src = y * (stride + 1) + 1;
    const row = y * stride;
    const prev = row - stride;
    for (let x = 0; x < stride; x++) {
      const v = raw[src + x] ?? 0;
      const a = x >= bpp ? (out[row + x - bpp] ?? 0) : 0;
      const b = y > 0 ? (out[prev + x] ?? 0) : 0;
      const c = y > 0 && x >= bpp ? (out[prev + x - bpp] ?? 0) : 0;
      let value: number;
      switch (filter) {
        case 0:
          value = v;
          break;
        case 1:
          value = v + a;
          break;
        case 2:
          value = v + b;
          break;
        case 3:
          value = v + ((a + b) >> 1);
          break;
        case 4:
          value = v + paeth(a, b, c);
          break;
        default:
          fail('png.bad_filter', { filter, row: y });
      }
      out[row + x] = value & 0xff;
    }
  }
  return out;
}

export function decodePng(
  data: Uint8Array,
  format: PngFormat,
  width: number,
  height: number,
): PngPixels {
  const kind = KINDS[format];
  const r = new ByteReader(data);
  const sig = r.bytesView(8);
  if (SIGNATURE.some((b, i) => sig[i] !== b)) fail('png.bad_signature');
  const idat: Uint8Array[] = [];
  let header = false;
  let ended = false;
  while (!ended) {
    if (r.remaining === 0) fail('png.truncated');
    const length = r.u32be();
    const type = chunkType(r);
    const body = r.slice(r.offset, length);
    r.skip(length);
    r.skip(4); // CRC, ignored.
    if (!header) {
      if (type !== 'IHDR' || length < 13) fail('png.missing_ihdr');
      const w = body.u32be();
      const h = body.u32be();
      const depth = body.u8();
      const colorType = body.u8();
      body.u8();
      body.u8();
      const interlace = body.u8();
      if (depth !== 8 || interlace !== 0) fail('png.unsupported', { depth, interlace });
      if (w !== width || h !== height || colorType !== kind.colorType) {
        fail('png.format_mismatch', { width: w, height: h, color_type: colorType });
      }
      header = true;
    } else if (type === 'IDAT') {
      idat.push(body.bytesView(length));
    } else if (type === 'IEND') {
      ended = true;
    }
  }
  if (idat.length === 0) fail('png.missing_idat');
  const total = idat.reduce((sum, part) => sum + part.length, 0);
  const joined = new Uint8Array(total);
  let at = 0;
  for (const part of idat) {
    joined.set(part, at);
    at += part.length;
  }
  const raw = inflate(joined, height * (1 + width * kind.bpp));
  return { width, height, bpp: kind.bpp, pixels: unfilter(raw, width, height, kind.bpp) };
}
