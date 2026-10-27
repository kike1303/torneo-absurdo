// RLE8 of SFF v2 (format 2): `01rrrrrr` plus a color is a run, anything else a literal.
import { fail } from './errors.ts';
import { ByteReader } from './reader.ts';

export function decodeRle8(data: Uint8Array, size: number): Uint8Array {
  const input = new ByteReader(data);
  const out = new Uint8Array(size);
  let w = 0;
  while (w < size) {
    if (input.remaining === 0) fail('rle8.truncated', { written: w });
    const ch = input.u8();
    if ((ch & 0xc0) === 0x40) {
      if (input.remaining === 0) fail('rle8.truncated', { written: w });
      const color = input.u8();
      const n = Math.min(ch & 0x3f, size - w);
      out.fill(color, w, w + n);
      w += n;
    } else {
      out[w++] = ch;
    }
  }
  return out;
}
