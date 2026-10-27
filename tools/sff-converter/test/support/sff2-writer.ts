// SFF v2 writer (002 RF-B6): header, sprite and palette nodes, `ldata` and `tdata`.
import { ByteWriter } from './bytes.ts';

export interface Sff2Sprite {
  readonly group: number;
  readonly image: number;
  readonly width: number;
  readonly height: number;
  readonly axisX: number;
  readonly axisY: number;
  /** 0 raw, 2 RLE8, 3 RLE5, 4 LZ5, 10–12 PNG. */
  readonly format: number;
  readonly depth?: number;
  /** Full data as stored (the u32 size prefix included), or null for a linked sprite. */
  readonly data: Uint8Array | null;
  readonly linked?: number;
  readonly palette?: number;
  readonly tdata?: boolean;
}

export interface Sff2Palette {
  readonly colors: number;
  /** `colors × 4` bytes, or null for a linked palette. */
  readonly data: Uint8Array | null;
  readonly linked?: number;
}

export interface Sff2Spec {
  readonly minor?: number;
  readonly sprites: readonly Sff2Sprite[];
  readonly palettes: readonly Sff2Palette[];
}

/** The u32 uncompressed size followed by the compressed bytes (RLE8, LZ5 and PNG). */
export function withSize(size: number, data: Uint8Array): Uint8Array {
  return new ByteWriter().u32(size).bytes(data).done();
}

export function writeSffV2(spec: Sff2Spec): Uint8Array {
  const ldata = new ByteWriter();
  const tdata = new ByteWriter();
  const paletteOffsets = spec.palettes.map((p) => {
    if (p.data === null) return 0;
    const at = ldata.length;
    ldata.bytes(p.data);
    return at;
  });
  const spriteOffsets = spec.sprites.map((s) => {
    if (s.data === null) return 0;
    const block = s.tdata ? tdata : ldata;
    const at = block.length;
    block.bytes(s.data);
    return at;
  });
  const spriteNodes = 512;
  const paletteNodes = spriteNodes + spec.sprites.length * 28;
  const ldataOffset = paletteNodes + spec.palettes.length * 16;
  const tdataOffset = ldataOffset + ldata.length;

  const w = new ByteWriter();
  w.ascii('ElecbyteSpr\0')
    .u8(0)
    .u8(spec.minor ?? 1)
    .u8(0)
    .u8(2);
  w.zeros(20);
  w.u32(spriteNodes).u32(spec.sprites.length).u32(paletteNodes).u32(spec.palettes.length);
  w.u32(ldataOffset).u32(ldata.length).u32(tdataOffset).u32(tdata.length);
  w.zeros(512 - w.length);
  spec.sprites.forEach((s, i) => {
    w.u16(s.group).u16(s.image).u16(s.width).u16(s.height).i16(s.axisX).i16(s.axisY);
    w.u16(s.linked ?? 0)
      .u8(s.format)
      .u8(s.depth ?? 8);
    w.u32(spriteOffsets[i] ?? 0)
      .u32(s.data?.length ?? 0)
      .u16(s.palette ?? 0)
      .u16(s.tdata ? 1 : 0);
  });
  spec.palettes.forEach((p, i) => {
    w.u16(1)
      .u16(i)
      .u16(p.colors)
      .u16(p.linked ?? 0);
    w.u32(paletteOffsets[i] ?? 0).u32(p.data?.length ?? 0);
  });
  w.bytes(ldata.done()).bytes(tdata.done());
  return w.done();
}

/** A palette of `colors` entries: index i → (i, 255 − i, i / 2, 0). */
export function testPaletteV2(colors: number): Uint8Array {
  const p = new Uint8Array(colors * 4);
  for (let i = 0; i < colors; i++) {
    p[i * 4] = i;
    p[i * 4 + 1] = 255 - i;
    p[i * 4 + 2] = i >> 1;
  }
  return p;
}
