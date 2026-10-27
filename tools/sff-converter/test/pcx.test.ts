import { describe, expect, it } from 'vitest';
import { decodePcx, pcxPalette, readPcxHeader } from '../src/pcx.ts';
import { code } from './support/expect-error.ts';
import { testPaletteRgb, writePcx } from './support/pcx-writer.ts';

// RF-B3 and RF-B4: PCX of SFF v1.
describe('decodePcx', () => {
  it('decodes literals and runs, with bytesPerLine > width', () => {
    const pcx = writePcx({
      width: 3,
      height: 2,
      bytesPerLine: 4,
      packets: [{ literal: 1 }, { run: 2, value: 0xc5 }, { literal: 0 }, { run: 4, value: 7 }],
    });
    const { header, indices } = decodePcx(pcx);
    expect(header).toMatchObject({ width: 3, height: 2, bytesPerLine: 4 });
    expect([...indices]).toEqual([1, 0xc5, 0xc5, 0, 7, 7, 7, 7]);
  });

  it('a run that crosses the end of the output is clipped, never written beyond', () => {
    const pcx = writePcx({
      width: 2,
      height: 1,
      bytesPerLine: 2,
      packets: [{ run: 63, value: 9 }],
    });
    expect([...decodePcx(pcx).indices]).toEqual([9, 9]);
  });

  it('input that ends before the output is full is pcx.truncated', () => {
    const pcx = writePcx({ width: 4, height: 2, bytesPerLine: 4, packets: [{ run: 3, value: 1 }] });
    expect(code(() => decodePcx(pcx))).toBe('pcx.truncated');
    // A run header without its value.
    const half = writePcx({ width: 4, height: 1, bytesPerLine: 4, packets: [{ literal: 1 }] });
    expect(code(() => decodePcx(Uint8Array.of(...half, 0xc3)))).toBe('pcx.truncated');
  });

  it('reads the 768-byte palette after 0x0C, and nothing when it is absent', () => {
    const palette = testPaletteRgb();
    const pcx = writePcx({
      width: 1,
      height: 1,
      bytesPerLine: 1,
      packets: [{ literal: 3 }],
      palette,
    });
    expect([...(pcxPalette(pcx) ?? [])]).toEqual([...palette]);
    expect(decodePcx(pcx).indices[0]).toBe(3);
    expect(pcxPalette(writePcx({ width: 1, height: 1, packets: [{ literal: 3 }] }))).toBeNull();
  });

  it('24-bit or multi-plane PCX is pcx.unsupported', () => {
    expect(code(() => decodePcx(writePcx({ width: 1, height: 1, planes: 3, packets: [] })))).toBe(
      'pcx.unsupported',
    );
    expect(
      code(() => decodePcx(writePcx({ width: 1, height: 1, bitsPerPixel: 4, packets: [] }))),
    ).toBe('pcx.unsupported');
  });

  it('a size beyond 1024 or a stride below the width is rejected', () => {
    expect(code(() => decodePcx(writePcx({ width: 1025, height: 1, packets: [] })))).toBe(
      'sff.bad_sprite_size',
    );
    expect(
      code(() => decodePcx(writePcx({ width: 4, height: 1, bytesPerLine: 3, packets: [] }))),
    ).toBe('pcx.bad_header');
    expect(code(() => readPcxHeader(new Uint8Array(100)))).toBe('sff.out_of_range');
  });
});
