// A reference sprite set for the guide tests, built in memory (007 plan-a decision 7): a valid
// manifest for validateManifest and validateGuide, and its sheet as RGBA pixels, without PNG.
import { readFileSync } from 'node:fs';
import { parseRulesetTiming, type RgbaImage, type RulesetTiming } from '../../src/guide/index.ts';
import { validateManifest, type SpriteManifest } from '../../src/manifest/index.ts';

export const CELL = 192;
export const CELLS = 4;
export const PIVOT = { x: 96, y: 184 } as const;
/** The body: 40 px wide, rows 64 to 183 (height 120, feet on pivotY − 1). */
export const BODY = { x: 76, w: 40, top: 64, bottom: 183 } as const;
/** The arm of cell 3 reaches this column (reach 40 ≥ CONTACT_PX / 2). */
export const ARM = { top: 120, bottom: 127, right: 136 } as const;

const anim = (frames: number[], frame_ms: number[], loop: boolean, impact_frame?: number) =>
  impact_frame === undefined
    ? { frames, frame_ms, loop }
    : { frames, frame_ms, loop, impact_frame };

/** The manifest as unvalidated JSON, to patch with applyOps. */
export function refManifest(): Record<string, unknown> {
  return {
    format: 'torneo.sprites',
    format_version: 1,
    id: 'ref_guide',
    pixel_art: true,
    sheets: [{ file: 'sheet-0.png', width: CELL * CELLS, height: CELL }],
    frames: Array.from({ length: CELLS }, (_, i) => ({
      sheet: 0,
      x: i * CELL,
      y: 0,
      w: CELL,
      h: CELL,
      pivot_x: PIVOT.x,
      pivot_y: PIVOT.y,
    })),
    animations: {
      idle: anim([0, 1, 2, 1], [200, 250, 200, 250], true),
      walk: anim([0, 1, 0, 1], [110, 110, 110, 110], true),
      intro: anim([0, 1, 2], [200, 200, 200], false),
      attack_light: anim([0, 0, 3, 0], [60, 60, 90, 120], false, 2),
      attack_heavy: anim([0, 0, 0, 3, 0], [100, 100, 100, 150, 220], false, 3),
      special: anim([0, 0, 0, 0, 3, 0], [80, 80, 80, 80, 160, 260], false, 4),
      hit: anim([0, 1, 0], [80, 120, 200], false),
      block: anim([0, 1], [100, 250], false),
      dodge: anim([0, 1, 0], [80, 160, 160], false),
      knockdown: anim([0, 1, 2, 0], [100, 150, 250, 400], false),
      ko: anim([0, 1, 2, 0], [100, 150, 250, 500], false),
      win: anim([0, 1, 0, 1], [200, 200, 200, 200], true),
    },
  };
}

export interface CellPainter {
  /** Sets the alpha (and a fixed opaque color) of a rectangle, in cell coordinates. */
  rect(x: number, y: number, w: number, h: number, alpha?: number): void;
  clear(): void;
}

export function paint(sheets: readonly RgbaImage[], cell: number, fn: (p: CellPainter) => void) {
  const image = sheets[0];
  if (image === undefined) throw new Error('no sheet');
  const set = (x: number, y: number, alpha: number) => {
    const i = (y * image.width + cell * CELL + x) * 4;
    image.data[i] = alpha === 0 ? 0 : 200;
    image.data[i + 1] = alpha === 0 ? 0 : 120;
    image.data[i + 2] = alpha === 0 ? 0 : 40;
    image.data[i + 3] = alpha;
  };
  fn({
    rect(x, y, w, h, alpha = 255) {
      for (let yy = y; yy < y + h; yy++) for (let xx = x; xx < x + w; xx++) set(xx, yy, alpha);
    },
    clear() {
      for (let y = 0; y < CELL; y++) for (let x = 0; x < CELL; x++) set(x, y, 0);
    },
  });
}

/** The body with its top at `top` (and its height kept). */
export function body(p: CellPainter, top: number = BODY.top): void {
  p.rect(BODY.x, top, BODY.w, BODY.bottom - BODY.top + 1);
}

/** A fresh copy of the reference pixels every call. */
export function refSheets(): RgbaImage[] {
  const sheets: RgbaImage[] = [
    { width: CELL * CELLS, height: CELL, data: new Uint8Array(CELL * CELLS * CELL * 4) },
  ];
  // F0, F1 and F2: the body in idle, one pixel apart (three distinct frames, same box).
  paint(sheets, 0, (p) => body(p));
  paint(sheets, 1, (p) => {
    body(p);
    p.rect(BODY.x, 100, 1, 1, 0);
  });
  paint(sheets, 2, (p) => {
    body(p);
    p.rect(BODY.x + 1, 100, 1, 1, 0);
  });
  // F3: the body with the arm out.
  paint(sheets, 3, (p) => {
    body(p);
    p.rect(BODY.x + BODY.w, ARM.top, ARM.right - BODY.x - BODY.w + 1, ARM.bottom - ARM.top + 1);
  });
  return sheets;
}

/** Validates a manifest JSON; the guide only takes validated manifests. */
export function validated(json: unknown): SpriteManifest {
  const result = validateManifest(json);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

/** The ruleset golden written by crates/content, read as bytes (no JSON import across packages). */
export function goldenJson(): unknown {
  const url = new URL(
    '../../../../crates/content/tests/golden/ruleset-timing.json',
    import.meta.url,
  );
  return JSON.parse(readFileSync(url, 'utf8'));
}

function parsedGolden(): readonly RulesetTiming[] {
  const result = parseRulesetTiming(goldenJson());
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.rulesets;
}

export const GOLDEN: readonly RulesetTiming[] = parsedGolden();

/** Ruleset v1, from the real golden. */
export const V1: RulesetTiming = (() => {
  const v1 = GOLDEN.find((r) => r.ruleset_version === 1);
  if (v1 === undefined) throw new Error('ruleset v1 missing from the golden');
  return v1;
})();

/** A synthetic later version, with V1's timings unless overridden. */
export function v2(overrides: Partial<RulesetTiming> = {}): RulesetTiming {
  return { ...V1, ruleset_version: 2, ...overrides };
}
