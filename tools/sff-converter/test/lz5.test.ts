import { describe, expect, it } from 'vitest';
import { decodeLz5 } from '../src/lz5.ts';
import { code } from './support/expect-error.ts';
import { writeLz5, type Lz5Packet } from './support/lz5-writer.ts';

// One vector per packet kind (plan-b step 5).
describe('decodeLz5', () => {
  it('short RLE: 3 bits of run and 5 of color', () => {
    expect([...decodeLz5(writeLz5([{ kind: 'rle', color: 17, run: 3 }]), 3)]).toEqual([17, 17, 17]);
  });

  it('long RLE: run = next + 8', () => {
    const out = decodeLz5(writeLz5([{ kind: 'rle_long', color: 5, run: 20 }]), 20);
    expect([...out]).toEqual(new Array(20).fill(5));
  });

  it('short LZ with the offset of every fourth packet rebuilt from recycled bits', () => {
    const packets: Lz5Packet[] = [
      { kind: 'rle', color: 1, run: 1 },
      { kind: 'rle', color: 2, run: 1 },
      { kind: 'rle', color: 3, run: 1 },
      { kind: 'rle', color: 4, run: 1 },
      { kind: 'lz', offset: 4, length: 2 }, // 1 2
      { kind: 'lz', offset: 2, length: 2 }, // 1 2
      { kind: 'lz', offset: 1, length: 3 }, // 2 2 2
      // Fourth short LZ: its offset (here 0b10_01_11_01 + 1 = 158) only travels in the high bits.
      { kind: 'rle_long', color: 9, run: 150 },
      { kind: 'lz', offset: 158, length: 4 },
    ];
    const size = 4 + 2 + 2 + 3 + 150 + 4;
    const out = decodeLz5(writeLz5(packets), size);
    expect([...out.subarray(0, 11)]).toEqual([1, 2, 3, 4, 1, 2, 1, 2, 2, 2, 2]);
    // 158 back from position 161 is position 3 onwards: 4, 1, 2, 1.
    expect([...out.subarray(161)]).toEqual([4, 1, 2, 1]);
  });

  it('long LZ: 10-bit offset and length = next + 3', () => {
    const packets: Lz5Packet[] = [
      { kind: 'rle', color: 7, run: 1 },
      { kind: 'rle_long', color: 0, run: 263 },
      { kind: 'rle_long', color: 3, run: 263 },
      { kind: 'lz_long', offset: 527, length: 5 },
    ];
    const out = decodeLz5(writeLz5(packets), 1 + 263 + 263 + 5);
    expect([...out.subarray(527)]).toEqual([7, 0, 0, 0, 0]);
  });

  it('an overlapping copy repeats the pattern', () => {
    const out = decodeLz5(
      writeLz5([
        { kind: 'rle', color: 1, run: 1 },
        { kind: 'rle', color: 2, run: 1 },
        { kind: 'lz', offset: 2, length: 6 },
      ]),
      8,
    );
    expect([...out]).toEqual([1, 2, 1, 2, 1, 2, 1, 2]);
  });

  it('an offset beyond what was written is lz5.bad_offset', () => {
    expect(
      code(() =>
        decodeLz5(
          writeLz5([
            { kind: 'rle', color: 1, run: 1 },
            { kind: 'lz', offset: 5, length: 2 },
          ]),
          3,
        ),
      ),
    ).toBe('lz5.bad_offset');
  });

  it('input that runs out is lz5.truncated', () => {
    expect(code(() => decodeLz5(writeLz5([{ kind: 'rle', color: 1, run: 2 }]), 4))).toBe(
      'lz5.truncated',
    );
    expect(code(() => decodeLz5(new Uint8Array(0), 1))).toBe('lz5.truncated');
  });

  it('a run past the end is clipped', () => {
    expect([...decodeLz5(writeLz5([{ kind: 'rle', color: 1, run: 7 }]), 2)]).toEqual([1, 1]);
  });
});
