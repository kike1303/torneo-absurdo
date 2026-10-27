import { describe, expect, it } from 'vitest';
import { decodePng } from '../src/png.ts';
import { code } from './support/expect-error.ts';
import { writePng } from './support/png-writer.ts';

const W = 5;
const H = 5;
const rgba = Uint8Array.from({ length: W * H * 4 }, (_, i) => (i * 37 + (i >> 3) * 11) & 0xff);

describe('decodePng', () => {
  it('undoes the five filters (RGBA)', () => {
    const png = writePng({
      width: W,
      height: H,
      colorType: 6,
      pixels: rgba,
      filters: [0, 1, 2, 3, 4],
    });
    expect([...decodePng(png, 12, W, H).pixels]).toEqual([...rgba]);
  });

  it('keeps the indices of PNG8 and ignores the embedded PLTE and the CRC', () => {
    const indices = Uint8Array.from({ length: W * H }, (_, i) => i % 7);
    const png = writePng({
      width: W,
      height: H,
      colorType: 3,
      pixels: indices,
      filters: [4, 3, 2, 1, 0],
      plte: new Uint8Array(21),
      idatChunks: 3,
    });
    expect([...decodePng(png, 10, W, H).pixels]).toEqual([...indices]);
  });

  it('PNG24 is RGB', () => {
    const rgb = rgba.subarray(0, W * H * 3);
    expect([
      ...decodePng(
        writePng({ width: W, height: H, colorType: 2, pixels: rgb, filters: [2] }),
        11,
        W,
        H,
      ).pixels,
    ]).toEqual([...rgb]);
  });

  it('an IHDR that does not match the node is png.format_mismatch', () => {
    const base = { width: W, height: H, colorType: 6, pixels: rgba } as const;
    expect(code(() => decodePng(writePng({ ...base, ihdr: { width: 4 } }), 12, W, H))).toBe(
      'png.format_mismatch',
    );
    expect(code(() => decodePng(writePng(base), 10, W, H))).toBe('png.format_mismatch');
  });

  it('interlaced or 16-bit PNG is png.unsupported', () => {
    const base = { width: W, height: H, colorType: 6, pixels: rgba } as const;
    expect(code(() => decodePng(writePng({ ...base, interlace: 1 }), 12, W, H))).toBe(
      'png.unsupported',
    );
    expect(code(() => decodePng(writePng({ ...base, depth: 16 }), 12, W, H))).toBe(
      'png.unsupported',
    );
  });

  it('IDAT that inflates to more than expected is png.too_large; less is png.size_mismatch', () => {
    const base = { width: W, height: H, colorType: 6, pixels: rgba } as const;
    expect(code(() => decodePng(writePng({ ...base, extra: 4096 }), 12, W, H))).toBe(
      'png.too_large',
    );
    const small = writePng({
      width: W,
      height: H - 1,
      colorType: 6,
      pixels: rgba,
      ihdr: { height: H },
    });
    expect(code(() => decodePng(small, 12, W, H))).toBe('png.size_mismatch');
  });

  it('an unknown filter is png.bad_filter; garbage IDAT is png.corrupt', () => {
    const base = { width: W, height: H, colorType: 6, pixels: rgba } as const;
    expect(code(() => decodePng(writePng({ ...base, filters: [5] }), 12, W, H))).toBe(
      'png.bad_filter',
    );
    const png = writePng(base);
    const idat = png.indexOf(0x49, 33); // 'I' of the IDAT type
    const broken = Uint8Array.from(png);
    broken.fill(0xff, idat + 4, idat + 10);
    expect(code(() => decodePng(broken, 12, W, H))).toBe('png.corrupt');
  });

  it('a bad signature, a missing IHDR and a truncated file fail with codes', () => {
    const png = writePng({ width: W, height: H, colorType: 6, pixels: rgba });
    expect(code(() => decodePng(new Uint8Array(8), 12, W, H))).toBe('png.bad_signature');
    expect(code(() => decodePng(png.subarray(0, 20), 12, W, H))).toBe('sff.out_of_range');
    expect(code(() => decodePng(png.subarray(0, png.length - 12), 12, W, H))).toBe('png.truncated');
  });
});
