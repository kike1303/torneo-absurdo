import { describe, expect, it } from 'vitest';
import { decodeRle8 } from '../src/rle8.ts';
import { code } from './support/expect-error.ts';

describe('decodeRle8', () => {
  it('runs are 01rrrrrr plus a color; anything else is a literal', () => {
    expect([...decodeRle8(Uint8Array.of(0x43, 5, 0x80, 0x01, 0xc0), 6)]).toEqual([
      5, 5, 5, 0x80, 1, 0xc0,
    ]);
  });

  it('clips a run to the output and ignores trailing bytes', () => {
    expect([...decodeRle8(Uint8Array.of(0x7f, 2, 9, 9), 3)]).toEqual([2, 2, 2]);
  });

  it('input that ends early is rle8.truncated', () => {
    expect(code(() => decodeRle8(Uint8Array.of(1), 2))).toBe('rle8.truncated');
    expect(code(() => decodeRle8(Uint8Array.of(0x42), 2))).toBe('rle8.truncated');
  });
});
