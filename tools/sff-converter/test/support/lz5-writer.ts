// LZ5 writer at packet level (002 RF-B6). Each packet kind of the decoder has its own encoding,
// including the high bits that short LZ packets recycle for every fourth one's offset.
import { ByteWriter } from './bytes.ts';

export type Lz5Packet =
  | { readonly kind: 'rle'; readonly color: number; readonly run: number } // run 1–7
  | { readonly kind: 'rle_long'; readonly color: number; readonly run: number } // run 8–263
  | { readonly kind: 'lz'; readonly offset: number; readonly length: number } // length 2–64
  | { readonly kind: 'lz_long'; readonly offset: number; readonly length: number }; // length 3–258, offset ≤ 1024

export function writeLz5(packets: readonly Lz5Packet[]): Uint8Array {
  // High bits of each short LZ: chunks of (offset − 1) of the fourth packet of its group.
  const shortIdx = packets.flatMap((p, i) => (p.kind === 'lz' ? [i] : []));
  const high = new Map<number, number>();
  for (let g = 0; g + 3 < shortIdx.length; g += 4) {
    const fourth = packets[shortIdx[g + 3] ?? 0];
    const v = fourth !== undefined && fourth.kind === 'lz' ? fourth.offset - 1 : 0;
    for (let m = 0; m < 4; m++) high.set(shortIdx[g + m] ?? 0, ((v >> (6 - 2 * m)) & 3) << 6);
  }
  const fourths = new Set<number>();
  for (let g = 3; g < shortIdx.length; g += 4) fourths.add(shortIdx[g] ?? 0);

  const w = new ByteWriter();
  for (let start = 0; start < packets.length; start += 8) {
    const group = packets.slice(start, start + 8);
    let control = 0;
    group.forEach((p, i) => {
      if (p.kind === 'lz' || p.kind === 'lz_long') control |= 1 << i;
    });
    w.u8(control);
    group.forEach((p, i) => {
      const at = start + i;
      switch (p.kind) {
        case 'rle':
          w.u8((p.run << 5) | (p.color & 0x1f));
          break;
        case 'rle_long':
          w.u8(p.color & 0x1f).u8(p.run - 8);
          break;
        case 'lz':
          w.u8((high.get(at) ?? 0) | (p.length - 1));
          if (!fourths.has(at)) w.u8(p.offset - 1);
          break;
        case 'lz_long': {
          const o = p.offset - 1;
          w.u8(((o >> 8) & 3) << 6)
            .u8(o & 0xff)
            .u8(p.length - 3);
          break;
        }
      }
    });
  }
  return w.done();
}
