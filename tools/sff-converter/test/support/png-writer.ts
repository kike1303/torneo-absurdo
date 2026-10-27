// PNG writer with a filter chosen per row (002 RF-B6). zlib only deflates; filtering is explicit.
import { deflateSync } from 'node:zlib';
import { ByteWriter, concat } from './bytes.ts';

export interface PngSpec {
  readonly width: number;
  readonly height: number;
  /** 3 indexed, 2 RGB, 6 RGBA. */
  readonly colorType: number;
  /** Unfiltered pixels, `width × bpp` per row. */
  readonly pixels: Uint8Array;
  /** Filter per row (0–4, or anything else to test the error); cycles if shorter. */
  readonly filters?: readonly number[];
  readonly depth?: number;
  readonly interlace?: number;
  /** IHDR overrides (a size or color type that does not match the node). */
  readonly ihdr?: { width?: number; height?: number; colorType?: number };
  /** Split IDAT into this many chunks. */
  readonly idatChunks?: number;
  /** Bytes appended to the raw data before deflating (inflates to more than expected). */
  readonly extra?: number;
  /** A PLTE chunk (ignored by the decoder: the node palette wins). */
  readonly plte?: Uint8Array;
}

const BPP: Readonly<Record<number, number>> = { 3: 1, 2: 3, 6: 4 };

function paeth(a: number, b: number, c: number): number {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) return a;
  return pb <= pc ? b : c;
}

export function filterRows(
  pixels: Uint8Array,
  width: number,
  height: number,
  bpp: number,
  filters: readonly number[],
): Uint8Array {
  const stride = width * bpp;
  const out = new Uint8Array(height * (stride + 1));
  for (let y = 0; y < height; y++) {
    const f = filters[y % Math.max(1, filters.length)] ?? 0;
    out[y * (stride + 1)] = f;
    for (let x = 0; x < stride; x++) {
      const v = pixels[y * stride + x] ?? 0;
      const a = x >= bpp ? (pixels[y * stride + x - bpp] ?? 0) : 0;
      const b = y > 0 ? (pixels[(y - 1) * stride + x] ?? 0) : 0;
      const c = y > 0 && x >= bpp ? (pixels[(y - 1) * stride + x - bpp] ?? 0) : 0;
      const pred =
        f === 1 ? a : f === 2 ? b : f === 3 ? (a + b) >> 1 : f === 4 ? paeth(a, b, c) : 0;
      out[y * (stride + 1) + 1 + x] = (v - pred) & 0xff;
    }
  }
  return out;
}

function chunk(type: string, data: Uint8Array): Uint8Array {
  // The CRC is written as zeros: the decoder ignores it (plan-b decision 7).
  return new ByteWriter().u32be(data.length).ascii(type).bytes(data).u32(0).done();
}

export function writePng(spec: PngSpec): Uint8Array {
  const bpp = BPP[spec.colorType] ?? 1;
  const ihdr = new ByteWriter()
    .u32be(spec.ihdr?.width ?? spec.width)
    .u32be(spec.ihdr?.height ?? spec.height)
    .u8(spec.depth ?? 8)
    .u8(spec.ihdr?.colorType ?? spec.colorType)
    .u8(0)
    .u8(0)
    .u8(spec.interlace ?? 0)
    .done();
  let raw = filterRows(spec.pixels, spec.width, spec.height, bpp, spec.filters ?? [0]);
  if (spec.extra !== undefined) raw = concat(raw, new Uint8Array(spec.extra));
  const z = new Uint8Array(deflateSync(raw));
  const n = Math.max(1, spec.idatChunks ?? 1);
  const size = Math.ceil(z.length / n);
  const idats: Uint8Array[] = [];
  for (let i = 0; i < n; i++) idats.push(chunk('IDAT', z.subarray(i * size, (i + 1) * size)));
  return concat(
    Uint8Array.of(0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a),
    chunk('IHDR', ihdr),
    ...(spec.plte !== undefined ? [chunk('PLTE', spec.plte)] : []),
    ...idats,
    chunk('IEND', new Uint8Array(0)),
  );
}
