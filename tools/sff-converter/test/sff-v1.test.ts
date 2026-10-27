import { describe, expect, it } from 'vitest';
import { decodeSprite, readSff, spriteIndexer, spriteInfo } from '../src/sff.ts';
import { code, converterError } from './support/expect-error.ts';
import { fixtureSffV1, spriteRgba } from './support/fixtures.ts';
import { pcxPacketsFor, testPaletteRgb, writePcx } from './support/pcx-writer.ts';
import { writeSffV1, type Sff1Sprite } from './support/sff1-writer.ts';

function pcx(rows: number[][], palette?: Uint8Array): Uint8Array {
  const w = rows[0]?.length ?? 1;
  return writePcx({
    width: w,
    height: rows.length,
    bytesPerLine: w,
    packets: pcxPacketsFor(rows, w),
    ...(palette ? { palette } : {}),
  });
}

function sprite(image: number, extra: Partial<Sff1Sprite> = {}): Sff1Sprite {
  return { group: 9, image, axisX: 0, axisY: 0, pcx: pcx([[1, 2]]), ...extra };
}

/** A palette where every color is (v, v, v). */
function flat(v: number): Uint8Array {
  return new Uint8Array(768).fill(v);
}

describe('SFF v1', () => {
  it('reads the header, the version and the chain of subfiles', () => {
    const file = readSff(fixtureSffV1());
    expect(file.version).toBe(1);
    expect(file.versionLabel).toBe('1.01');
    expect(file.sprites).toHaveLength(6);
    expect(spriteIndexer(file)(0, 3)).toBe(3);
    expect(spriteIndexer(file)(1, 0)).toBe(-1);
    expect(spriteInfo(file, 2)).toEqual({
      width: 10,
      height: 12,
      compression: 'pcx',
      supported: true,
    });
  });

  it('decodes with its own palette and "same palette" uses the previous one', () => {
    const file = readSff(fixtureSffV1());
    for (const k of [0, 4]) {
      const expected = spriteRgba(k);
      expect(decodeSprite(file, k)).toEqual(expected);
    }
  });

  it('a sprite with its own palette and without "same palette" uses its own', () => {
    const file = readSff(
      writeSffV1({
        sprites: [
          sprite(0, { pcx: pcx([[1]], flat(10)) }),
          sprite(1, { pcx: pcx([[1]], flat(20)) }),
          sprite(2, { pcx: pcx([[1]]), samePalette: true }),
        ],
      }),
    );
    expect([...decodeSprite(file, 1).rgba]).toEqual([20, 20, 20, 255]);
    expect([...decodeSprite(file, 2).rgba]).toEqual([20, 20, 20, 255]);
  });

  it('shared mode: "same palette" sprites take the palette of palette_sprite', () => {
    const file = readSff(
      writeSffV1({
        sharedPalette: true,
        sprites: [
          sprite(0, { pcx: pcx([[1]], flat(10)) }),
          sprite(1, { pcx: pcx([[1]], flat(30)) }),
          sprite(2, { pcx: pcx([[1]]), samePalette: true }),
        ],
      }),
    );
    expect([...decodeSprite(file, 2, { paletteSprite: 0 }).rgba]).toEqual([10, 10, 10, 255]);
    expect([...decodeSprite(file, 2, { paletteSprite: 1 }).rgba]).toEqual([30, 30, 30, 255]);
    expect(code(() => decodeSprite(file, 2))).toBe('sff.missing_palette');
  });

  it('without any palette it is sff.missing_palette', () => {
    const file = readSff(writeSffV1({ sprites: [sprite(0)] }));
    expect(code(() => decodeSprite(file, 0))).toBe('sff.missing_palette');
  });

  it('index 0 is transparent', () => {
    const file = readSff(
      writeSffV1({ sprites: [sprite(0, { pcx: pcx([[0, 5]], testPaletteRgb()) })] }),
    );
    expect([...decodeSprite(file, 0).rgba]).toEqual([0, 255, 0, 0, 5, 250, 2, 255]);
  });

  it('a linked sprite uses the data of its target', () => {
    const file = readSff(
      writeSffV1({
        sprites: [
          sprite(0, { pcx: pcx([[3, 4]], testPaletteRgb()) }),
          sprite(1, { pcx: null, linked: 0, samePalette: true }),
        ],
      }),
    );
    expect(decodeSprite(file, 1)).toEqual(decodeSprite(file, 0));
  });

  it('a cycle of links is sff.link_cycle; a link out of range is sff.link_out_of_range', () => {
    const cyclic = readSff(
      writeSffV1({
        sprites: [sprite(0, { pcx: null, linked: 1 }), sprite(1, { pcx: null, linked: 0 })],
      }),
    );
    expect(code(() => decodeSprite(cyclic, 0))).toBe('sff.link_cycle');
    expect(
      code(() => readSff(writeSffV1({ sprites: [sprite(0, { pcx: null, linked: 7 })] }))),
    ).toBe('sff.link_out_of_range');
  });

  it('a cyclic subfile list is sff.offset_cycle', () => {
    const bytes = writeSffV1({
      sprites: [sprite(0), sprite(1), sprite(2)],
      nextOverride: { 1: 512 },
    });
    expect(converterError(() => readSff(bytes)).code).toBe('sff.offset_cycle');
  });

  it('more sprites than the limit, or than the file holds, fail with codes', () => {
    expect(code(() => readSff(writeSffV1({ sprites: [sprite(0)], count: 10_001 })))).toBe(
      'sff.too_many_sprites',
    );
    expect(code(() => readSff(writeSffV1({ sprites: [sprite(0)], count: 3 })))).toBe(
      'sff.out_of_range',
    );
  });

  it('a 24-bit PCX is reported unsupported and fails only when decoded', () => {
    const file = readSff(
      writeSffV1({
        sprites: [sprite(0, { pcx: writePcx({ width: 1, height: 1, planes: 3, packets: [] }) })],
      }),
    );
    expect(spriteInfo(file, 0).supported).toBe(false);
    expect(code(() => decodeSprite(file, 0))).toBe('pcx.unsupported');
  });

  it('a bad signature or version is rejected', () => {
    const bytes = fixtureSffV1();
    const bad = Uint8Array.from(bytes);
    bad[0] = 0;
    expect(code(() => readSff(bad))).toBe('sff.bad_signature');
    const v3 = Uint8Array.from(bytes);
    v3[15] = 3;
    expect(code(() => readSff(v3))).toBe('sff.unsupported_version');
  });
});
