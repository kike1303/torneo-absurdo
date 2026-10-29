import { describe, expect, it } from 'vitest';
import { readIhdr } from '../src/ihdr.ts';
import { decodePng, encodeInterlaced, encodePng } from './support/png.ts';

// 007 plan-b decision 11: the header is read without decoding anything.
describe('readIhdr', () => {
  it('reads size, depth, color type and interlace', () => {
    const png = encodePng(3, 2, new Uint8Array(3 * 2 * 4));
    expect(readIhdr(png)).toEqual({ width: 3, height: 2, bitDepth: 8, colorType: 6, interlace: 0 });
    expect(readIhdr(encodePng(3, 2, new Uint8Array(24), { colorType: 2 }))?.colorType).toBe(2);
    expect(
      readIhdr(encodePng(3, 2, new Uint8Array(24), { colorType: 6, bitDepth: 16 })),
    ).toMatchObject({ bitDepth: 16 });
  });

  it('reads the interlace of the hand-built Adam7 helper, which decodes to the same pixels', () => {
    const rgba = new Uint8Array(13 * 11 * 4).map((_, i) => (i * 37) % 256);
    const png = encodeInterlaced(13, 11, rgba);
    expect(readIhdr(png)?.interlace).toBe(1);
    expect(Buffer.from(decodePng(png).data).equals(Buffer.from(rgba))).toBe(true);
  });

  it('is null on a bad signature, a short file or a first chunk that is not IHDR', () => {
    const png = encodePng(3, 2, new Uint8Array(24));
    expect(readIhdr(new Uint8Array(0))).toBeNull();
    expect(readIhdr(png.subarray(0, 20))).toBeNull();
    const badSignature = Buffer.from(png);
    badSignature[1] = 0;
    expect(readIhdr(badSignature)).toBeNull();
    const badType = Buffer.from(png);
    badType[12] = 0x58;
    expect(readIhdr(badType)).toBeNull();
    const badLength = Buffer.from(png);
    badLength[11] = 12;
    expect(readIhdr(badLength)).toBeNull();
  });
});
