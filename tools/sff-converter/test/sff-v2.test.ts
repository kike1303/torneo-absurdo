import { describe, expect, it } from 'vitest';
import { decodeSprite, readSff, spriteInfo } from '../src/sff.ts';
import { code } from './support/expect-error.ts';
import {
  fixtureSffV2,
  fixtureSpritesV2,
  HEIGHT,
  spriteRgba,
  spriteWidth,
} from './support/fixtures.ts';
import { writeLz5 } from './support/lz5-writer.ts';
import { testPaletteV2, withSize, writeSffV2, type Sff2Sprite } from './support/sff2-writer.ts';

const PALETTE = { colors: 32, data: testPaletteV2(32) };

function single(extra: Partial<Sff2Sprite>, palettes = [PALETTE]): Uint8Array {
  return writeSffV2({
    sprites: [
      {
        group: 0,
        image: 0,
        width: 2,
        height: 1,
        axisX: 0,
        axisY: 0,
        format: 0,
        data: Uint8Array.of(1, 2),
        ...extra,
      },
    ],
    palettes,
  });
}

describe('SFF v2', () => {
  it('reads version 2.00 and 2.01', () => {
    expect(readSff(writeSffV2({ minor: 0, sprites: [], palettes: [] })).versionLabel).toBe('2.00');
    expect(readSff(fixtureSffV2()).versionLabel).toBe('2.01');
  });

  it('decodes one sprite per format (raw, RLE8, LZ5, PNG8, PNG24, PNG32), from ldata and tdata', () => {
    const file = readSff(fixtureSffV2());
    const formats = fixtureSpritesV2().map((s) => s.format);
    expect(formats).toEqual([0, 2, 4, 10, 11, 12]);
    for (let k = 0; k < 6; k++) {
      const expected = spriteRgba(k);
      const got = decodeSprite(file, k);
      if (formats[k] === 11) {
        // PNG24 is opaque: same colors, alpha 255 everywhere.
        for (let i = 3; i < expected.rgba.length; i += 4) expected.rgba[i] = 255;
      }
      expect(got, `sprite ${k}`).toEqual(expected);
    }
  });

  it('a linked sprite and a linked palette resolve to their targets', () => {
    const file = readSff(
      writeSffV2({
        sprites: [
          {
            group: 0,
            image: 0,
            width: 2,
            height: 1,
            axisX: 0,
            axisY: 0,
            format: 0,
            data: Uint8Array.of(1, 2),
            palette: 1,
          },
          {
            group: 0,
            image: 1,
            width: 2,
            height: 1,
            axisX: 0,
            axisY: 0,
            format: 0,
            data: null,
            linked: 0,
          },
        ],
        palettes: [PALETTE, { colors: 0, data: null, linked: 0 }],
      }),
    );
    expect(decodeSprite(file, 1)).toEqual(decodeSprite(file, 0));
    expect([...decodeSprite(file, 0).rgba]).toEqual([1, 254, 0, 255, 2, 253, 1, 255]);
  });

  it('numcols < 256: an index beyond it is sff.palette_index_out_of_range', () => {
    const file = readSff(
      single({ data: Uint8Array.of(1, 7) }, [{ colors: 4, data: testPaletteV2(4) }]),
    );
    expect(code(() => decodeSprite(file, 0))).toBe('sff.palette_index_out_of_range');
  });

  it('palette data must be numcols × 4 bytes', () => {
    const file = readSff(single({}, [{ colors: 4, data: testPaletteV2(3) }]));
    expect(code(() => decodeSprite(file, 0))).toBe('sff.bad_palette');
    expect(code(() => decodeSprite(readSff(single({ palette: 3 })), 0))).toBe(
      'sff.palette_not_found',
    );
  });

  it('RLE5 and unknown formats: reported, an error only when decoded', () => {
    const file = readSff(
      writeSffV2({
        sprites: [
          {
            group: 0,
            image: 0,
            width: 2,
            height: 1,
            axisX: 0,
            axisY: 0,
            format: 3,
            data: withSize(2, Uint8Array.of(0)),
          },
          {
            group: 0,
            image: 1,
            width: 2,
            height: 1,
            axisX: 0,
            axisY: 0,
            format: 99,
            data: Uint8Array.of(0),
          },
        ],
        palettes: [PALETTE],
      }),
    );
    expect(spriteInfo(file, 0)).toMatchObject({ compression: 'rle5', supported: false });
    expect(spriteInfo(file, 1)).toMatchObject({ compression: 'unknown', supported: false });
    expect(code(() => decodeSprite(file, 0))).toBe('sff.unsupported_compression');
  });

  it('a declared size that does not match w × h is sff.size_mismatch', () => {
    const lz5 = writeLz5([{ kind: 'rle', color: 1, run: 2 }]);
    expect(
      code(() => decodeSprite(readSff(single({ format: 4, data: withSize(3, lz5) })), 0)),
    ).toBe('sff.size_mismatch');
    expect(code(() => decodeSprite(readSff(single({ data: Uint8Array.of(1) })), 0))).toBe(
      'sff.size_mismatch',
    );
  });

  it('a sprite larger than 1024 or of size 0 is sff.bad_sprite_size', () => {
    expect(code(() => decodeSprite(readSff(single({ width: 1025 })), 0))).toBe(
      'sff.bad_sprite_size',
    );
    expect(code(() => decodeSprite(readSff(single({ height: 0 })), 0))).toBe('sff.bad_sprite_size');
  });

  it('a node table or data block outside the file is sff.out_of_range', () => {
    const bytes = fixtureSffV2();
    expect(code(() => readSff(bytes.subarray(0, 600)))).toBe('sff.out_of_range');
    expect(spriteWidth(0) * HEIGHT).toBe(96);
  });
});
