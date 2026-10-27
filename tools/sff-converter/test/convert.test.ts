import { validateManifest } from '@torneo/renderer/manifest';
import { describe, expect, it } from 'vitest';
import { convert, type ConvertResult } from '../src/convert.ts';
import { parseMap } from '../src/map.ts';
import { code } from './support/expect-error.ts';
import { writeAir } from './support/air-writer.ts';
import {
  ACTIONS,
  AXIS,
  fixtureAir,
  fixtureMap,
  fixtureSffV1,
  fixtureSffV2,
  fixtureSpritesV2,
  MAP_JSON,
  spriteRgba,
} from './support/fixtures.ts';
import { writeSffV2 } from './support/sff2-writer.ts';

function ok(result: ConvertResult) {
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result;
}

/** The pixels of manifest frame `i` read back from its sheet. */
function framePixels(result: ReturnType<typeof ok>, i: number): Uint8Array {
  const f = result.manifest.frames[i]!;
  const sheet = result.sheets[f.sheet]!;
  const out = new Uint8Array(f.w * f.h * 4);
  for (let y = 0; y < f.h; y++) {
    const src = ((f.y + y) * sheet.width + f.x) * 4;
    out.set(sheet.rgba.subarray(src, src + f.w * 4), y * f.w * 4);
  }
  return out;
}

/** Sprite k without its transparent left column (what the trim leaves). */
function trimmed(k: number, opaque = false): Uint8Array {
  const s = spriteRgba(k);
  if (opaque) {
    for (let i = 3; i < s.rgba.length; i += 4) s.rgba[i] = 255;
    return s.rgba;
  }
  const w = s.width - 1;
  const out = new Uint8Array(w * s.height * 4);
  for (let y = 0; y < s.height; y++)
    out.set(s.rgba.subarray((y * s.width + 1) * 4, (y * s.width + s.width) * 4), y * w * 4);
  return out;
}

describe('convert (RF-B5)', () => {
  for (const [name, sff] of [
    ['v1', fixtureSffV1()],
    ['v2', fixtureSffV2()],
  ] as const) {
    it(`the synthetic ${name} character passes validateManifest with its id`, () => {
      const result = ok(convert({ sff, air: fixtureAir(), map: fixtureMap() }));
      expect(validateManifest(result.manifest, { expectedId: 'local_test' }).ok).toBe(true);
      expect(Object.keys(result.manifest.animations).sort()).toHaveLength(12);
      expect(result.manifest.pixel_art).toBe(true);
      expect(result.report).toMatchObject({ id: 'local_test', sprites: { total: 6, decoded: 6 } });
      expect(JSON.stringify(result.report)).not.toMatch(/\//);
    });
  }

  it('the pixels of each frame in the sheet match the source sprite (palette and mirror applied)', () => {
    const result = ok(convert({ sff: fixtureSffV1(), air: fixtureAir(), map: fixtureMap() }));
    const idle0 = result.manifest.animations.idle.frames[0]!; // sprite 1 (after Loopstart)
    expect(framePixels(result, idle0)).toEqual(trimmed(1));
    const f = result.manifest.frames[idle0]!;
    expect([f.pivot_x, f.pivot_y]).toEqual([AXIS.x - 1, AXIS.y]);
    // walk: sprite 3, then sprite 3 mirrored.
    const [plain, mirrored] = result.manifest.animations.walk!.frames;
    const a = framePixels(result, plain!);
    const b = framePixels(result, mirrored!);
    const w = result.manifest.frames[plain!]!.w;
    for (let y = 0; y < 12; y++) {
      for (let x = 0; x < w; x++) {
        expect([...b.subarray((y * w + x) * 4, (y * w + x) * 4 + 4)]).toEqual([
          ...a.subarray((y * w + (w - 1 - x)) * 4, (y * w + (w - 1 - x)) * 4 + 4),
        ]);
      }
    }
  });

  it('every v2 format lands in the sheet with the same pixels', () => {
    const result = ok(convert({ sff: fixtureSffV2(), air: fixtureAir(), map: fixtureMap() }));
    // attack_light frame 0 is sprite 1 (RLE8), attack_heavy frame 0 is sprite 2 (LZ5).
    expect(framePixels(result, result.manifest.animations.attack_light.frames[0]!)).toEqual(
      trimmed(1),
    );
    expect(framePixels(result, result.manifest.animations.attack_heavy.frames[0]!)).toEqual(
      trimmed(2),
    );
    // block is sprite 3 (PNG8); special frame 0 is sprite 4 (PNG24, opaque: no trim).
    expect(framePixels(result, result.manifest.animations.block.frames[0]!)).toEqual(trimmed(3));
    expect(framePixels(result, result.manifest.animations.special.frames[0]!)).toEqual(
      trimmed(4, true),
    );
  });

  it('frame_ms, holds, loops, empty frames and dedupe follow decision 9', () => {
    const result = ok(convert({ sff: fixtureSffV1(), air: fixtureAir(), map: fixtureMap() }));
    const a = result.manifest.animations;
    expect(a.idle).toMatchObject({ loop: true, frame_ms: [100, 100] });
    expect(a.attack_light).toMatchObject({ loop: false, frame_ms: [50, 67, 100], impact_frame: 1 });
    expect(a.ko).toMatchObject({ loop: false, frame_ms: [167, 100] }); // -1 held, the rest dropped
    const dodgeEmpty = result.manifest.frames[a.dodge.frames[1]!]!;
    expect([dodgeEmpty.w, dodgeEmpty.h, dodgeEmpty.pivot_x, dodgeEmpty.pivot_y]).toEqual([
      1, 1, 0, 0,
    ]);
    // idle frame 1 (sprite 2) and win frame 1 (sprite 2, held) share one frame.
    expect(a.win.frames[1]).toBe(a.idle.frames[1]);
    expect(result.report.notes.map((n) => n.code)).toContain('convert.unreachable_frames');
  });

  it('clamped durations and truncated animations carry their notes', () => {
    const actions = ACTIONS.map((act) =>
      act.id === 180
        ? {
            id: 180,
            frames: [
              { group: 0, image: 1, ticks: 500 },
              { group: 0, image: 2, ticks: 0 },
              { group: 0, image: 3, ticks: 120 },
              { group: 0, image: 4, ticks: 120 },
            ],
          }
        : act,
    );
    const result = ok(convert({ sff: fixtureSffV1(), air: writeAir(actions), map: fixtureMap() }));
    expect(result.manifest.animations.win.frame_ms).toEqual([2000, 17]);
    const codes = result.report.notes.map((n) => n.code);
    expect(codes).toEqual(
      expect.arrayContaining([
        'convert.frame_ms_clamped',
        'convert.zero_ticks',
        'convert.animation_truncated',
      ]),
    );
  });

  it('a windup over 400 ms ends with the ManifestError of validateManifest', () => {
    const map = parseMap({
      ...MAP_JSON,
      impact_frames: { ...MAP_JSON.impact_frames, attack_heavy: 2 },
    });
    const actions = ACTIONS.map((act) =>
      act.id === 210
        ? {
            id: 210,
            frames: [
              { group: 0, image: 1, ticks: 20 },
              { group: 0, image: 2, ticks: 20 },
              { group: 0, image: 3, ticks: 4 },
            ],
          }
        : act,
    );
    const result = convert({ sff: fixtureSffV1(), air: writeAir(actions), map });
    expect(result.ok).toBe(false);
    if (!result.ok) {
      expect(result.errors.map((e) => e.code)).toContain('manifest.windup_too_long');
      expect(result.report.manifest_errors).toEqual(result.errors);
    }
  });

  it('a missing required action fails; a missing optional one is a note', () => {
    const noKo = ACTIONS.filter((a) => a.id !== 5150);
    expect(
      code(() => convert({ sff: fixtureSffV1(), air: writeAir(noKo), map: fixtureMap() })),
    ).toBe('air.missing_action');
    const noWalk = ACTIONS.filter((a) => a.id !== 20);
    const result = ok(convert({ sff: fixtureSffV1(), air: writeAir(noWalk), map: fixtureMap() }));
    expect(result.manifest.animations.walk).toBeUndefined();
    expect(result.report.notes.map((n) => n.code)).toContain('convert.optional_missing');
  });

  it('an unsupported sprite nobody uses is fine; one a mapped animation uses fails', () => {
    const extra = {
      group: 7,
      image: 0,
      width: 2,
      height: 1,
      axisX: 0,
      axisY: 0,
      format: 3,
      data: Uint8Array.of(2, 0, 0, 0, 0),
    };
    const sff = writeSffV2({
      sprites: [...fixtureSpritesV2(), extra],
      palettes: [{ colors: 32, data: new Uint8Array(128) }],
    });
    const unused = ok(convert({ sff, air: fixtureAir(), map: fixtureMap() }));
    expect(unused.report.notes).toContainEqual({
      code: 'sff.unsupported_compression',
      count: 1,
      data: [{ sprite: 6, compression: 'rle5' }],
    });
    const air = writeAir(
      ACTIONS.map((a) =>
        a.id === 130 ? { id: 130, frames: [{ group: 7, image: 0, ticks: 5 }] } : a,
      ),
    );
    expect(code(() => convert({ sff, air, map: fixtureMap() }))).toBe(
      'sff.unsupported_compression',
    );
  });
});
