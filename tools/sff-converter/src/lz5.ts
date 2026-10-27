// LZ5 of SFF v2 (format 4), per Elecbyte's formats.txt (plan-b "Alcance de formatos").
// A control byte gives 8 flags, least significant first: 0 is an RLE packet, 1 an LZ packet.
import { fail } from './errors.ts';
import { ByteReader } from './reader.ts';

export function decodeLz5(data: Uint8Array, size: number): Uint8Array {
  const input = new ByteReader(data);
  const out = new Uint8Array(size);
  const next = (w: number): number => {
    if (input.remaining === 0) fail('lz5.truncated', { written: w });
    return input.u8();
  };
  let w = 0;
  let control = 0;
  let flag = 8;
  let recycled = 0;
  let recycledBits = 0;
  while (w < size) {
    if (flag === 8) {
      control = next(w);
      flag = 0;
    }
    const d = next(w);
    if ((control & (1 << flag)) !== 0) {
      let offset: number;
      let length: number;
      if ((d & 0x3f) === 0) {
        // Long LZ: 10-bit offset, length = next + 3.
        offset = (((d & 0xc0) << 2) | next(w)) + 1;
        length = next(w) + 3;
      } else {
        // Short LZ: the two high bits are recycled; every fourth packet takes its offset from them.
        recycled |= (d & 0xc0) >> recycledBits;
        recycledBits += 2;
        length = (d & 0x3f) + 1;
        if (recycledBits < 8) {
          offset = next(w) + 1;
        } else {
          offset = recycled + 1;
          recycled = 0;
          recycledBits = 0;
        }
      }
      if (offset > w) fail('lz5.bad_offset', { offset, written: w });
      const n = Math.min(length, size - w);
      for (let i = 0; i < n; i++, w++) out[w] = out[w - offset] ?? 0;
    } else {
      let color: number;
      let run: number;
      if ((d & 0xe0) === 0) {
        color = d & 0x1f;
        run = next(w) + 8;
      } else {
        color = d & 0x1f;
        run = d >> 5;
      }
      const n = Math.min(run, size - w);
      out.fill(color, w, w + n);
      w += n;
    }
    flag++;
  }
  return out;
}
