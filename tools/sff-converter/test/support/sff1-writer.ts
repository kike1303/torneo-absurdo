// SFF v1 writer (002 RF-B6): the 512-byte header and the chain of 32-byte subheaders.
import { ByteWriter } from './bytes.ts';

export interface Sff1Sprite {
  readonly group: number;
  readonly image: number;
  readonly axisX: number;
  readonly axisY: number;
  /** PCX bytes, or null for a linked sprite. */
  readonly pcx: Uint8Array | null;
  readonly linked?: number;
  readonly samePalette?: boolean;
}

export interface Sff1Spec {
  readonly sharedPalette?: boolean;
  readonly sprites: readonly Sff1Sprite[];
  /** Override of the next offset of a subfile (to build cycles). */
  readonly nextOverride?: Readonly<Record<number, number>>;
  readonly count?: number;
}

export function writeSffV1(spec: Sff1Spec): Uint8Array {
  const w = new ByteWriter();
  w.ascii('ElecbyteSpr\0').u8(0).u8(1).u8(0).u8(1);
  w.u32(spec.sprites.length)
    .u32(spec.count ?? spec.sprites.length)
    .u32(512)
    .u32(32);
  w.u8(spec.sharedPalette ? 1 : 0);
  w.zeros(512 - w.length);
  spec.sprites.forEach((s, i) => {
    const start = w.length;
    const length = s.pcx?.length ?? 0;
    const next = spec.nextOverride?.[i] ?? start + 32 + length;
    w.u32(next).u32(length).i16(s.axisX).i16(s.axisY).u16(s.group).u16(s.image);
    w.u16(s.linked ?? 0)
      .u8(s.samePalette ? 1 : 0)
      .zeros(13);
    if (s.pcx !== null) w.bytes(s.pcx);
  });
  return w.done();
}
