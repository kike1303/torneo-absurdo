import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { PNG } from 'pngjs';
import { describe, expect, it } from 'vitest';
import {
  GUIDE_CODES,
  validateGuide,
  type GuideError,
  type RgbaImage,
  type RulesetTiming,
} from '../src/guide/index.ts';
import type { SpriteManifest } from '../src/manifest/index.ts';
import {
  ARM,
  BODY,
  CELL,
  body,
  paint,
  refManifest,
  refSheets,
  v2,
  validated,
  V1,
  type CellPainter,
} from './support/guide-ref.ts';
import { applyOps, type PatchOp } from './support/patch.ts';
import { DUMMY_A, DUMMY_B } from './support/placeholders.ts';

// 007 RF-A3 and RF-A4: the reference passes, and every rule has a case that fails with exactly its
// code, its path and its data.
const set = (path: string, value: unknown): PatchOp => ({ op: 'set', path, value });
const ms = (anim: string, value: number[]) => set(`/animations/${anim}/frame_ms`, value);

interface Case {
  readonly name: string;
  readonly ops?: readonly PatchOp[];
  readonly pixels?: (sheets: RgbaImage[]) => void;
  readonly rulesets?: readonly RulesetTiming[];
  readonly errors: readonly GuideError[];
}

function run(c: Omit<Case, 'name' | 'errors'>) {
  const manifest = validated(applyOps(refManifest(), c.ops ?? []));
  const sheets = refSheets();
  c.pixels?.(sheets);
  return validateGuide(manifest, sheets, c.rulesets ?? [V1]);
}

const cell = (n: number, fn: (p: CellPainter) => void) => (sheets: RgbaImage[]) =>
  paint(sheets, n, fn);

const CASES: readonly Case[] = [
  { name: 'no_rulesets', rulesets: [], errors: [{ code: 'guide.no_rulesets', path: '' }] },
  {
    name: 'scale_forbidden',
    ops: [set('/scale', 1)],
    errors: [{ code: 'guide.scale_forbidden', path: '/scale' }],
  },
  {
    name: 'cell_size',
    ops: [set('/frames/2/w', 190)],
    errors: [{ code: 'guide.cell_size', path: '/frames/2', data: { w: 190, h: 192 } }],
  },
  {
    name: 'pivot_position',
    ops: [set('/frames/2/pivot_x', 95)],
    errors: [
      { code: 'guide.pivot_position', path: '/frames/2', data: { pivot_x: 95, pivot_y: 184 } },
    ],
  },
  {
    name: 'frame_ms_range',
    ops: [set('/animations/win/frame_ms/1', 20)],
    errors: [
      { code: 'guide.frame_ms_range', path: '/animations/win/frame_ms/1', data: { ms: 20 } },
    ],
  },
  {
    name: 'frames_count',
    ops: [set('/animations/win/frames', [0]), ms('win', [200])],
    errors: [
      {
        code: 'guide.frames_count',
        path: '/animations/win/frames',
        data: { count: 1, min: 2, max: 12 },
      },
    ],
  },
  {
    name: 'idle_distinct',
    ops: [set('/animations/idle/frames', [0, 1, 0, 1])],
    errors: [{ code: 'guide.idle_distinct', path: '/animations/idle/frames', data: { count: 2 } }],
  },
  {
    name: 'idle_distinct (same pixels under two indices)',
    pixels: cell(2, (p) => {
      p.clear();
      body(p);
      p.rect(BODY.x, 100, 1, 1, 0);
    }),
    errors: [{ code: 'guide.idle_distinct', path: '/animations/idle/frames', data: { count: 2 } }],
  },
  {
    name: 'idle_frame_long',
    ops: [ms('idle', [300, 200, 200, 200])],
    errors: [
      { code: 'guide.idle_frame_long', path: '/animations/idle/frame_ms/0', data: { ms: 300 } },
    ],
  },
  {
    name: 'cycle_range',
    ops: [ms('walk', [60, 60, 60, 60])],
    errors: [
      {
        code: 'guide.cycle_range',
        path: '/animations/walk/frame_ms',
        data: { total_ms: 240, min: 300, max: 1200 },
      },
    ],
  },
  {
    name: 'duration_range (hit)',
    ops: [set('/animations/hit/frames', [0, 1]), ms('hit', [80, 80])],
    errors: [
      {
        code: 'guide.duration_range',
        path: '/animations/hit/frame_ms',
        data: { total_ms: 160, min: 200, max: 1000 },
      },
    ],
  },
  {
    name: 'duration_range (intro)',
    ops: [ms('intro', [600, 600, 600])],
    errors: [
      {
        code: 'guide.duration_range',
        path: '/animations/intro/frame_ms',
        data: { total_ms: 1800, min: 0, max: 1600 },
      },
    ],
  },
  {
    name: 'duration_range (knockdown)',
    ops: [ms('knockdown', [400, 400, 400, 400])],
    errors: [
      {
        code: 'guide.duration_range',
        path: '/animations/knockdown/frame_ms',
        data: { total_ms: 1600, min: 0, max: 1500 },
      },
    ],
  },
  {
    name: 'duration_range (knockdown, capped by a later version)',
    ops: [ms('knockdown', [400, 400, 400, 400])],
    rulesets: [V1, v2({ knockdown_ms: 1000 })],
    errors: [
      {
        code: 'guide.duration_range',
        path: '/animations/knockdown/frame_ms',
        data: { total_ms: 1600, min: 0, max: 1000 },
      },
    ],
  },
  {
    name: 'duration_range (ko)',
    ops: [ms('ko', [100, 500, 500, 500])],
    errors: [
      {
        code: 'guide.duration_range',
        path: '/animations/ko/frame_ms',
        data: { total_ms: 1600, min: 0, max: 1500 },
      },
    ],
  },
  {
    name: 'windup_short',
    ops: [ms('attack_light', [40, 40, 90, 120])],
    errors: [
      {
        code: 'guide.windup_short',
        path: '/animations/attack_light/impact_frame',
        data: { windup_ms: 80, min: 100 },
      },
    ],
  },
  {
    name: 'windup_long (guide)',
    ops: [
      ms('attack_light', [130, 130, 90, 120]),
      ms('attack_heavy', [100, 100, 180, 150, 220]),
      ms('special', [95, 95, 95, 95, 160, 260]),
    ],
    errors: [
      {
        code: 'guide.windup_long',
        path: '/animations/attack_light/impact_frame',
        data: { windup_ms: 260, cap_ms: 250 },
      },
    ],
  },
  {
    name: 'windup_long (ruleset)',
    rulesets: [V1, v2({ windup_heavy_ms: 280 })],
    errors: [
      {
        code: 'guide.windup_long',
        path: '/animations/attack_heavy/impact_frame',
        data: { windup_ms: 300, cap_ms: 280, ruleset_version: 2 },
      },
    ],
  },
  {
    name: 'anticipation',
    ops: [
      ms('attack_heavy', [300, 100, 100, 150, 220]),
      set('/animations/attack_heavy/impact_frame', 1),
      set('/animations/attack_heavy/frames', [0, 3, 0, 0, 0]),
    ],
    errors: [
      {
        code: 'guide.anticipation',
        path: '/animations/attack_heavy/impact_frame',
        data: { impact_frame: 1, min: 2 },
      },
    ],
  },
  {
    name: 'impact_hold',
    ops: [ms('attack_light', [60, 60, 40, 170])],
    errors: [
      {
        code: 'guide.impact_hold',
        path: '/animations/attack_light/frame_ms/2',
        data: { ms: 40, min: 50 },
      },
    ],
  },
  {
    name: 'recovery_short',
    ops: [ms('attack_light', [60, 60, 90, 100])],
    errors: [
      {
        code: 'guide.recovery_short',
        path: '/animations/attack_light/frame_ms',
        data: { recovery_ms: 190, min: 200 },
      },
    ],
  },
  {
    name: 'reaction_slow',
    ops: [ms('hit', [140, 60, 200])],
    errors: [
      {
        code: 'guide.reaction_slow',
        path: '/animations/hit/frame_ms/0',
        data: { ms: 140, max: 134 },
      },
    ],
  },
  {
    name: 'frame_empty (and no impact_short in cascade)',
    pixels: cell(3, (p) => p.clear()),
    errors: [{ code: 'guide.frame_empty', path: '/frames/3' }],
  },
  {
    name: 'frame_clipped',
    pixels: cell(2, (p) => p.rect(0, 0, 1, 1)),
    errors: [{ code: 'guide.frame_clipped', path: '/frames/2' }],
  },
  {
    name: 'feet_off_pivot (once, though idle uses the cell twice)',
    pixels: cell(1, (p) => {
      p.clear();
      body(p, BODY.top - 5);
      p.rect(BODY.x, 95, 1, 1, 0);
    }),
    errors: [
      { code: 'guide.feet_off_pivot', path: '/animations/idle/frames/1', data: { row: 178 } },
    ],
  },
  {
    name: 'height_range',
    pixels: cell(0, (p) => {
      p.clear();
      p.rect(BODY.x, 134, BODY.w, 50);
    }),
    errors: [
      { code: 'guide.height_range', path: '/animations/idle/frames/0', data: { height: 50 } },
    ],
  },
  {
    name: 'impact_short (one per attack)',
    pixels: cell(3, (p) => {
      p.clear();
      body(p);
      p.rect(BODY.x + BODY.w, ARM.top, 1, ARM.bottom - ARM.top + 1);
    }),
    errors: [
      {
        code: 'guide.impact_short',
        path: '/animations/attack_light/frames/2',
        data: { reach: 20 },
      },
      {
        code: 'guide.impact_short',
        path: '/animations/attack_heavy/frames/3',
        data: { reach: 20 },
      },
      { code: 'guide.impact_short', path: '/animations/special/frames/4', data: { reach: 20 } },
    ],
  },
  {
    name: 'too_many_errors',
    ops: [
      set(
        '/frames',
        Array.from({ length: 101 }, () => ({
          sheet: 0,
          x: 0,
          y: 0,
          w: CELL,
          h: CELL,
          pivot_x: 95,
          pivot_y: 184,
        })),
      ),
    ],
    errors: [
      ...Array.from({ length: 100 }, (_, i): GuideError => ({
        code: 'guide.pivot_position',
        path: `/frames/${i}`,
        data: { pivot_x: 95, pivot_y: 184 },
      })),
      { code: 'guide.too_many_errors', path: '' },
    ],
  },
];

const WINDUP_ORDER: readonly Case[] = [
  {
    name: 'light 250, heavy 300 (step < 100)',
    ops: [ms('attack_light', [125, 125, 90, 120])],
    errors: [
      {
        code: 'guide.windup_order',
        path: '/animations/attack_heavy/impact_frame',
        data: { light_ms: 250, heavy_ms: 300, special_ms: 320, min_step: 100 },
      },
    ],
  },
  {
    name: 'heavy 380 above special 320',
    ops: [ms('attack_heavy', [100, 100, 180, 150, 220])],
    errors: [
      {
        code: 'guide.windup_order',
        path: '/animations/special/impact_frame',
        data: { light_ms: 120, heavy_ms: 380, special_ms: 320, min_step: 100 },
      },
    ],
  },
  {
    name: 'both: light 250, heavy 340, special 320',
    ops: [ms('attack_light', [125, 125, 90, 120]), ms('attack_heavy', [100, 100, 140, 150, 220])],
    errors: [
      {
        code: 'guide.windup_order',
        path: '/animations/attack_heavy/impact_frame',
        data: { light_ms: 250, heavy_ms: 340, special_ms: 320, min_step: 100 },
      },
      {
        code: 'guide.windup_order',
        path: '/animations/special/impact_frame',
        data: { light_ms: 250, heavy_ms: 340, special_ms: 320, min_step: 100 },
      },
    ],
  },
];

describe('validateGuide', () => {
  it('passes the reference with v1 and with two versions', () => {
    expect(run({})).toEqual({ ok: true });
    expect(run({ rulesets: [V1, v2()] })).toEqual({ ok: true });
  });

  it.each(CASES.map((c) => [c.name, c] as const))('%s', (_name, c) => {
    expect(run(c)).toEqual({ ok: false, errors: c.errors });
  });

  it('covers every code with a case', () => {
    const covered = new Set(
      [...CASES, ...WINDUP_ORDER].flatMap((c) => c.errors.map((e) => e.code)),
    );
    expect(Object.values(GUIDE_CODES).filter((code) => !covered.has(code))).toEqual([]);
  });

  it('is deterministic and does not modify its input', () => {
    const manifest = validated(refManifest());
    const sheets = refSheets();
    paint(sheets, 3, (p) => p.clear());
    const before = { manifest: structuredClone(manifest), data: sheets[0]?.data.slice() };
    const first = validateGuide(manifest, sheets, [V1]);
    expect(validateGuide(manifest, sheets, [V1])).toEqual(first);
    expect(manifest).toEqual(before.manifest);
    expect(sheets[0]?.data).toEqual(before.data);
  });

  it('throws on pixels that do not match the sheets (a caller error)', () => {
    const manifest = validated(refManifest());
    const mismatch = new RangeError('guide.sheets_mismatch');
    expect(() => validateGuide(manifest, [], [V1])).toThrow(mismatch);
    const [sheet] = refSheets();
    if (!sheet) throw new Error('no sheet');
    expect(() =>
      validateGuide(manifest, [{ ...sheet, data: sheet.data.subarray(4) }], [V1]),
    ).toThrow(mismatch);
    expect(() => validateGuide(manifest, [{ ...sheet, width: sheet.width - 1 }], [V1])).toThrow(
      mismatch,
    );
  });
});

describe('guide.windup_order', () => {
  it('passes 120/300/320 (the reference) and the exact edge 200/300/300', () => {
    expect(run({})).toEqual({ ok: true });
    expect(
      run({
        ops: [ms('attack_light', [100, 100, 90, 120]), ms('special', [75, 75, 75, 75, 160, 260])],
      }),
    ).toEqual({ ok: true });
  });

  it.each(WINDUP_ORDER.map((c) => [c.name, c] as const))('%s', (_name, c) => {
    expect(run(c)).toEqual({ ok: false, errors: c.errors });
  });
});

describe('the placeholders stay out of the guide (plan-a decision 8)', () => {
  const dir = (id: string) => fileURLToPath(new URL(`../placeholders/${id}/`, import.meta.url));
  const decode = (id: string, manifest: SpriteManifest): RgbaImage[] =>
    manifest.sheets.map((sheet) => {
      const png = PNG.sync.read(readFileSync(`${dir(id)}${sheet.file}`));
      return { width: png.width, height: png.height, data: new Uint8Array(png.data) };
    });

  it.each([
    ['dummy_a', DUMMY_A, ['guide.cell_size']],
    ['dummy_b', DUMMY_B, ['guide.cell_size', 'guide.windup_order']],
  ] as const)('%s fails with another cell', (id, manifest, codes) => {
    const result = validateGuide(manifest, decode(id, manifest), [V1]);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect([...new Set(result.errors.map((e) => e.code))]).toEqual(codes);
    const cells = result.errors.filter((e) => e.code === 'guide.cell_size');
    expect(cells.map((e) => e.path)).toEqual(manifest.frames.map((_, i) => `/frames/${i}`));
  });
});

describe('guide.idle_distinct after the counts and before the durations (007 plan-b decisions 6 and 7)', () => {
  it('reports idle_distinct before idle_frame_long', () => {
    expect(
      run({
        ops: [set('/animations/idle/frames', [0, 1, 0, 1]), ms('idle', [300, 200, 200, 200])],
      }),
    ).toEqual({
      ok: false,
      errors: [
        { code: 'guide.idle_distinct', path: '/animations/idle/frames', data: { count: 2 } },
        { code: 'guide.idle_frame_long', path: '/animations/idle/frame_ms/0', data: { ms: 300 } },
      ],
    });
  });

  it('is not evaluated when an idle cell is empty (no cascade)', () => {
    expect(
      run({
        pixels: (sheets) => {
          paint(sheets, 1, (p) => p.clear());
          paint(sheets, 2, (p) => p.clear());
        },
      }),
    ).toEqual({
      ok: false,
      errors: [
        { code: 'guide.frame_empty', path: '/frames/1' },
        { code: 'guide.frame_empty', path: '/frames/2' },
      ],
    });
  });

  it('is not evaluated when an idle cell has another size', () => {
    expect(run({ ops: [set('/frames/1/w', 190)] })).toEqual({
      ok: false,
      errors: [{ code: 'guide.cell_size', path: '/frames/1', data: { w: 190, h: 192 } }],
    });
  });
});
