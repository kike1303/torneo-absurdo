import { describe, expect, it } from 'vitest';
import { ByteReader } from '../src/reader.ts';
import { code, converterError } from './support/expect-error.ts';

// RF-B4, plan-b decision 4.
describe('ByteReader', () => {
  const bytes = Uint8Array.of(1, 2, 3, 4, 0xff, 0xff, 0x00, 0x80);

  it('reads little endian and signed values', () => {
    const r = new ByteReader(bytes);
    expect(r.u16()).toBe(0x0201);
    expect(r.u16()).toBe(0x0403);
    expect(r.i16()).toBe(-1);
    expect(r.i16()).toBe(-0x8000);
    expect(new ByteReader(bytes).u32()).toBe(0x04030201);
    expect(new ByteReader(bytes).at(4).u32()).toBe(0x8000ffff);
    expect(new ByteReader(bytes).u32be()).toBe(0x01020304);
  });

  it('reads exactly up to the end and fails one byte beyond', () => {
    const r = new ByteReader(bytes).at(4);
    expect(r.u32()).toBe(0x8000ffff);
    expect(r.remaining).toBe(0);
    expect(code(() => r.u8())).toBe('sff.out_of_range');
    expect(code(() => new ByteReader(bytes).at(5).u32())).toBe('sff.out_of_range');
    expect(new ByteReader(bytes).at(8).remaining).toBe(0);
    expect(code(() => new ByteReader(bytes).at(9))).toBe('sff.out_of_range');
  });

  it('rejects negative and non-integer offsets and lengths', () => {
    const r = new ByteReader(bytes);
    for (const bad of [-1, 0.5, Number.NaN, Number.POSITIVE_INFINITY, 2 ** 60]) {
      expect(code(() => r.at(bad))).toBe('sff.out_of_range');
      expect(code(() => r.slice(bad, 1))).toBe('sff.out_of_range');
      expect(code(() => r.slice(0, bad))).toBe('sff.out_of_range');
    }
    expect(converterError(() => r.slice(6, 4)).data).toEqual({ offset: 6, length: 4 });
  });

  it('a slice never sees outside its window', () => {
    const s = new ByteReader(bytes).slice(2, 2);
    expect(s.length).toBe(2);
    expect(s.u16()).toBe(0x0403);
    expect(code(() => s.u8())).toBe('sff.out_of_range');
    expect(code(() => s.slice(1, 2))).toBe('sff.out_of_range');
  });
});
