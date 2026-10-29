// The reference export `ref_a` (007 RF-B8, plan-b decision 21): a geometric figure, no character
// and no brand, drawn with rectangles (body, head and arm), one fill color, a dark 1 px outline and
// binary alpha. Deterministic; it meets every rule of the guide. Its timings are those of the
// guide tests of phase A (windup 120/300/320).
import { GUIDE_LIMITS, type RgbaImage } from '@torneo/renderer/guide';
import type { AnimationCode } from '@torneo/renderer/manifest';

const CELL = GUIDE_LIMITS.cellPx;
const FILL = [200, 120, 40] as const;
const OUTLINE = [40, 24, 16] as const;

/** A pose: rectangles in cell coordinates, each filled and outlined. */
interface Rect {
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
}

/** The feet stand on row 183 (pivotY − 1); the body is 40 px wide around the pivot. */
const FEET = CELL - GUIDE_LIMITS.groundMarginPx - 1;
const BODY_X = 76;
const BODY_W = 40;
const HEAD_W = 24;

function figure(opts: {
  readonly dx?: number;
  readonly top?: number;
  readonly arm?: Rect;
}): Rect[] {
  const dx = opts.dx ?? 0;
  const top = opts.top ?? 88;
  const body: Rect = { x: BODY_X + dx, y: top, w: BODY_W, h: FEET - top + 1 };
  const head: Rect = {
    x: BODY_X + dx + (BODY_W - HEAD_W) / 2,
    y: top - HEAD_W,
    w: HEAD_W,
    h: HEAD_W,
  };
  return opts.arm === undefined ? [body, head] : [body, head, opts.arm];
}

const ARM_Y = 120;
const ARM_H = 8;
/** Behind the body (windup). */
const armBack = (dx = 0): Rect => ({ x: BODY_X + dx - 18, y: ARM_Y, w: 18, h: ARM_H });
/** Halfway (recovery). */
const armHalf = (dx = 0): Rect => ({ x: BODY_X + BODY_W + dx, y: ARM_Y, w: 10, h: ARM_H });
/** Extended to column 136 (reach 40 ≥ CONTACT_PX / 2). */
const armOut = (dx = 0): Rect => ({
  x: BODY_X + BODY_W + dx,
  y: ARM_Y,
  w: 137 - (BODY_X + BODY_W + dx),
  h: ARM_H,
});
/** Raised (block and win). */
const armUp = (dx = 0): Rect => ({ x: BODY_X + BODY_W + dx, y: 60, w: ARM_H, h: 60 });

/** Lying on the ground (knockdown and ko). */
const lying = (h: number): Rect[] => [
  { x: 40, y: FEET - h + 1, w: 112, h },
  { x: 152, y: FEET - 20, w: 22, h: 21 },
];

const POSES = {
  idle0: figure({}),
  idle1: figure({ top: 89 }),
  idle2: figure({ top: 87 }),
  step0: figure({ dx: -3 }),
  step1: figure({ dx: 3 }),
  windup: figure({ arm: armBack() }),
  windupDeep: figure({ dx: -4, arm: armBack(-4) }),
  impact: figure({ arm: armOut() }),
  recover: figure({ arm: armHalf() }),
  hit: figure({ dx: -8, top: 92 }),
  block: figure({ arm: armUp() }),
  dodge: figure({ dx: -6, top: 120 }),
  fall: figure({ dx: -10, top: 130 }),
  down: lying(30),
  flat: lying(24),
  cheer: figure({ top: 86, arm: armUp() }),
} as const satisfies Record<string, readonly Rect[]>;

type Pose = keyof typeof POSES;

interface RefAnimation {
  readonly poses: readonly Pose[];
  readonly frame_ms: readonly number[];
  readonly loop: boolean;
  readonly impact_frame?: number;
}

/** In canonical order (REQUIRED_ANIMATIONS, then OPTIONAL_ANIMATIONS). */
const ANIMATIONS: ReadonlyArray<readonly [AnimationCode, RefAnimation]> = [
  [
    'idle',
    { poses: ['idle0', 'idle1', 'idle2', 'idle1'], frame_ms: [200, 250, 200, 250], loop: true },
  ],
  [
    'attack_light',
    {
      poses: ['windup', 'windup', 'impact', 'recover'],
      frame_ms: [60, 60, 90, 120],
      loop: false,
      impact_frame: 2,
    },
  ],
  [
    'attack_heavy',
    {
      poses: ['windup', 'windupDeep', 'windupDeep', 'impact', 'recover'],
      frame_ms: [100, 100, 100, 150, 220],
      loop: false,
      impact_frame: 3,
    },
  ],
  [
    'special',
    {
      poses: ['windup', 'windupDeep', 'windupDeep', 'windup', 'impact', 'recover'],
      frame_ms: [80, 80, 80, 80, 160, 260],
      loop: false,
      impact_frame: 4,
    },
  ],
  ['hit', { poses: ['idle0', 'hit', 'idle0'], frame_ms: [80, 120, 200], loop: false }],
  ['block', { poses: ['idle0', 'block'], frame_ms: [100, 250], loop: false }],
  ['dodge', { poses: ['idle0', 'dodge', 'idle0'], frame_ms: [80, 160, 160], loop: false }],
  ['ko', { poses: ['hit', 'fall', 'down', 'flat'], frame_ms: [100, 150, 250, 500], loop: false }],
  [
    'win',
    { poses: ['idle0', 'cheer', 'idle0', 'cheer'], frame_ms: [200, 200, 200, 200], loop: true },
  ],
  ['intro', { poses: ['idle2', 'idle1', 'idle0'], frame_ms: [200, 200, 200], loop: false }],
  [
    'walk',
    { poses: ['step0', 'idle0', 'step1', 'idle0'], frame_ms: [110, 110, 110, 110], loop: true },
  ],
  [
    'knockdown',
    { poses: ['hit', 'fall', 'down', 'idle0'], frame_ms: [100, 150, 250, 400], loop: false },
  ],
];

function paint(rects: readonly Rect[]): RgbaImage {
  const data = new Uint8Array(CELL * CELL * 4);
  const set = (x: number, y: number, rgb: readonly [number, number, number]) => {
    const i = (y * CELL + x) * 4;
    data[i] = rgb[0];
    data[i + 1] = rgb[1];
    data[i + 2] = rgb[2];
    data[i + 3] = 255;
  };
  // Outlines first, then every fill: overlapping parts merge into one silhouette.
  for (const r of rects) {
    for (let y = r.y; y < r.y + r.h; y++) for (let x = r.x; x < r.x + r.w; x++) set(x, y, OUTLINE);
  }
  for (const r of rects) {
    for (let y = r.y + 1; y < r.y + r.h - 1; y++) {
      for (let x = r.x + 1; x < r.x + r.w - 1; x++) set(x, y, FILL);
    }
  }
  return { width: CELL, height: CELL, data };
}

export const REF_ID = 'ref_a';

export interface RefExport {
  readonly exportJson: Readonly<Record<string, unknown>>;
  /** `<animation>/<nn>` → pixels. */
  readonly frames: ReadonlyMap<string, RgbaImage>;
}

export function drawRef(): RefExport {
  const animations: Record<string, unknown> = {};
  const frames = new Map<string, RgbaImage>();
  for (const [code, anim] of ANIMATIONS) {
    animations[code] =
      anim.impact_frame === undefined
        ? { frame_ms: anim.frame_ms, loop: anim.loop }
        : { frame_ms: anim.frame_ms, loop: anim.loop, impact_frame: anim.impact_frame };
    anim.poses.forEach((pose, step) => {
      frames.set(`${code}/${String(step).padStart(2, '0')}`, paint(POSES[pose]));
    });
  }
  return {
    exportJson: {
      format: 'torneo.sprites.export',
      format_version: 1,
      id: REF_ID,
      pixel_art: true,
      animations,
    },
    frames,
  };
}
