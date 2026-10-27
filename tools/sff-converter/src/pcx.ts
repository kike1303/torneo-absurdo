// PCX of SFF v1 (002 RF-B3): 8 bits, 1 plane, RLE, optional 256-color palette at the end.
import { fail } from './errors.ts';
import { INPUT_LIMITS } from './limits.ts';
import { ByteReader } from './reader.ts';

export const PCX_HEADER = 128;
const PALETTE_TRAILER = 769;

export interface PcxHeader {
  readonly width: number;
  readonly height: number;
  readonly bytesPerLine: number;
  readonly bitsPerPixel: number;
  readonly planes: number;
  readonly encoding: number;
}

export function readPcxHeader(data: Uint8Array): PcxHeader {
  const r = new ByteReader(data);
  r.at(2);
  const encoding = r.u8();
  const bitsPerPixel = r.u8();
  const xmin = r.u16();
  const ymin = r.u16();
  const xmax = r.u16();
  const ymax = r.u16();
  r.at(65);
  const planes = r.u8();
  const bytesPerLine = r.u16();
  r.at(PCX_HEADER);
  return {
    width: xmax - xmin + 1,
    height: ymax - ymin + 1,
    bytesPerLine,
    bitsPerPixel,
    planes,
    encoding,
  };
}

export function isSupportedPcx(h: PcxHeader): boolean {
  return h.bitsPerPixel === 8 && h.planes === 1 && h.encoding === 1;
}

/** Checks the header for decoding: supported variant, sane size and stride. */
export function checkPcxHeader(h: PcxHeader): void {
  if (!isSupportedPcx(h)) {
    fail('pcx.unsupported', { bits: h.bitsPerPixel, planes: h.planes, encoding: h.encoding });
  }
  const max = INPUT_LIMITS.spriteSideMax;
  if (h.width < 1 || h.height < 1 || h.width > max || h.height > max) {
    fail('sff.bad_sprite_size', { width: h.width, height: h.height });
  }
  if (h.bytesPerLine < h.width || h.bytesPerLine > max + 2) {
    fail('pcx.bad_header', { bytes_per_line: h.bytesPerLine, width: h.width });
  }
}

/** The 768 RGB bytes of the trailer (`0x0C` plus the palette), or null. */
export function pcxPalette(data: Uint8Array): Uint8Array | null {
  if (data.length < PCX_HEADER + PALETTE_TRAILER) return null;
  const at = data.length - PALETTE_TRAILER;
  if (data[at] !== 0x0c) return null;
  return data.subarray(at + 1);
}

export interface PcxPixels {
  readonly header: PcxHeader;
  /** `bytesPerLine × height` indices. */
  readonly indices: Uint8Array;
}

export function decodePcx(data: Uint8Array): PcxPixels {
  const header = readPcxHeader(data);
  checkPcxHeader(header);
  const end = pcxPalette(data) === null ? data.length : data.length - PALETTE_TRAILER;
  const input = new ByteReader(data.subarray(PCX_HEADER, Math.max(PCX_HEADER, end)));
  const out = new Uint8Array(header.bytesPerLine * header.height);
  let w = 0;
  while (w < out.length) {
    if (input.remaining === 0) fail('pcx.truncated', { written: w });
    const b = input.u8();
    if ((b & 0xc0) === 0xc0) {
      const run = b & 0x3f;
      if (input.remaining === 0) fail('pcx.truncated', { written: w });
      const value = input.u8();
      const n = Math.min(run, out.length - w);
      out.fill(value, w, w + n);
      w += n;
    } else {
      out[w++] = b;
    }
  }
  return { header, indices: out };
}
