// PCX writer at packet level (002 RF-B6): runs and literals are explicit, nothing is compressed
// behind the test's back.
import { ByteWriter, concat } from './bytes.ts';

export type PcxPacket =
  { readonly literal: number } | { readonly run: number; readonly value: number };

export interface PcxSpec {
  readonly width: number;
  readonly height: number;
  readonly bytesPerLine?: number;
  readonly bitsPerPixel?: number;
  readonly planes?: number;
  readonly packets: readonly PcxPacket[];
  /** 768 RGB bytes appended after `0x0C`. */
  readonly palette?: Uint8Array;
}

export function writePcx(spec: PcxSpec): Uint8Array {
  const h = new ByteWriter();
  h.u8(0x0a)
    .u8(5)
    .u8(1)
    .u8(spec.bitsPerPixel ?? 8);
  h.u16(0)
    .u16(0)
    .u16(spec.width - 1)
    .u16(spec.height - 1);
  h.u16(72).u16(72).zeros(48).u8(0);
  h.u8(spec.planes ?? 1).u16(spec.bytesPerLine ?? spec.width + (spec.width % 2));
  h.zeros(128 - h.length);
  const body = new ByteWriter();
  for (const p of spec.packets) {
    if ('literal' in p) body.u8(p.literal);
    else body.u8(0xc0 | p.run).u8(p.value);
  }
  const parts = [h.done(), body.done()];
  if (spec.palette !== undefined) parts.push(Uint8Array.of(0x0c), spec.palette);
  return concat(...parts);
}

/** Packets for indexed rows: a run for every pixel value ≥ 0xC0 or repeated, a literal otherwise. */
export function pcxPacketsFor(
  rows: readonly (readonly number[])[],
  bytesPerLine: number,
): PcxPacket[] {
  const out: PcxPacket[] = [];
  for (const row of rows) {
    const line = [...row, ...new Array<number>(bytesPerLine - row.length).fill(0)];
    for (let i = 0; i < line.length;) {
      const v = line[i] ?? 0;
      let n = 1;
      while (i + n < line.length && line[i + n] === v && n < 63) n++;
      if (n > 1 || v >= 0xc0) out.push({ run: n, value: v });
      else out.push({ literal: v });
      i += n;
    }
  }
  return out;
}

/** A grayscale-ish deterministic palette (index i → i, 255 − i, i / 2). */
export function testPaletteRgb(): Uint8Array {
  const p = new Uint8Array(768);
  for (let i = 0; i < 256; i++) {
    p[i * 3] = i;
    p[i * 3 + 1] = 255 - i;
    p[i * 3 + 2] = i >> 1;
  }
  return p;
}
