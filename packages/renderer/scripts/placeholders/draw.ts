// Deterministic drawing of the placeholder fighters `dummy_a` and `dummy_b` (RF-A6, plan-a1
// decisions 11 and 12). Pure: returns the manifest and the RGBA pixels of each sheet; the
// generator script only writes them. Geometric figures, no character and no brand.
import type { AnimationCode, SpriteAnimation, SpriteManifest } from '../../src/manifest/index.ts';

export type PlaceholderId = 'dummy_a' | 'dummy_b';
export const PLACEHOLDER_IDS: readonly PlaceholderId[] = ['dummy_a', 'dummy_b'];

type Rgba = readonly [number, number, number, number];
type Point = readonly [number, number];

interface Palette {
  readonly body: Rgba;
  readonly dark: Rgba;
  readonly head: Rgba;
}

/** Drawing data only (not renderer colors): cold for `a`, warm for `b`. */
const PALETTES: Readonly<Record<PlaceholderId, Palette>> = {
  dummy_a: { body: [64, 140, 220, 255], dark: [36, 84, 150, 255], head: [160, 205, 240, 255] },
  dummy_b: { body: [226, 118, 48, 255], dark: [160, 74, 28, 255], head: [248, 196, 120, 255] },
};

interface Body {
  readonly frameW: number;
  readonly frameH: number;
  readonly pivotX: number;
  readonly pivotY: number;
  readonly legLen: number;
  readonly torsoLen: number;
  readonly torsoBottom: number;
  readonly torsoTop: number;
  readonly head: 'circle' | 'square';
  readonly headSize: number;
  readonly armLen: number;
  readonly armWidth: number;
  readonly legWidth: number;
}

/** `dummy_a`: tall rectangle with a round head. `dummy_b`: wide trapezoid with a square head. */
const BODIES: Readonly<Record<PlaceholderId, Body>> = {
  dummy_a: {
    frameW: 64,
    frameH: 96,
    pivotX: 32,
    pivotY: 92,
    legLen: 28,
    torsoLen: 30,
    torsoBottom: 16,
    torsoTop: 16,
    head: 'circle',
    headSize: 9,
    armLen: 24,
    armWidth: 6,
    legWidth: 6,
  },
  dummy_b: {
    frameW: 80,
    frameH: 88,
    pivotX: 40,
    pivotY: 84,
    legLen: 22,
    torsoLen: 26,
    torsoBottom: 20,
    torsoTop: 38,
    head: 'square',
    headSize: 16,
    armLen: 26,
    armWidth: 8,
    legWidth: 8,
  },
};

interface Pose {
  /** Torso rotation around the hip, radians; positive leans forward (right). */
  readonly lean?: number;
  /** 0..1, lowers the hip. */
  readonly crouch?: number;
  /** Arm angles from straight down, radians; positive swings forward, π/2 is horizontal. */
  readonly front?: number;
  readonly back?: number;
  readonly reach?: number;
  readonly backReach?: number;
  /** -1..1, spreads the feet back and forth. */
  readonly stride?: number;
  /** 0..1, falls backward until lying on the ground. */
  readonly fall?: number;
  readonly bob?: number;
  readonly dx?: number;
}

interface AnimationSpec {
  readonly poses: readonly Pose[];
  readonly frameMs: readonly number[];
  readonly loop: boolean;
  readonly impact?: number;
}

const H = Math.PI / 2;

function animations(id: PlaceholderId): Readonly<Record<AnimationCode, AnimationSpec>> {
  const heavyMs = id === 'dummy_a' ? [100, 100, 100, 150, 220] : [120, 120, 140, 150, 220];
  return {
    idle: {
      poses: [0, 1, 2, 1].map((bob) => ({ bob, front: 0.3, back: -0.2 })),
      frameMs: [150, 150, 150, 150],
      loop: true,
    },
    walk: {
      poses: [1, 0, -1, 0].map((stride) => ({ stride, front: -0.4 * stride, back: 0.4 * stride })),
      frameMs: [110, 110, 110, 110],
      loop: true,
    },
    intro: {
      poses: [
        { crouch: 1, front: 0.2 },
        { crouch: 0.7, front: 0.6 },
        { crouch: 0.4, front: 1.5 },
        { front: 2.6, back: -2.4 },
        { front: 2.8, back: -2.6, bob: -2 },
        { front: 0.3, back: -0.2 },
      ],
      frameMs: [150, 150, 150, 150, 200, 250],
      loop: false,
    },
    attack_light: {
      poses: [
        { lean: -0.1, front: -0.3 },
        { lean: -0.15, front: -0.6 },
        { lean: 0.15, front: H, reach: 1.3, stride: 0.6 },
        { lean: 0.05, front: 0.8 },
      ],
      frameMs: [60, 60, 90, 120],
      loop: false,
      impact: 2,
    },
    attack_heavy: {
      poses: [
        { lean: -0.15, front: -0.8 },
        { lean: -0.25, front: -1.2, crouch: 0.2 },
        { lean: -0.1, front: -0.6, crouch: 0.35 },
        { lean: 0.3, front: H, reach: 1.5, stride: 1 },
        { lean: 0.1, front: 0.9 },
      ],
      frameMs: heavyMs,
      loop: false,
      impact: 3,
    },
    special: {
      poses: [
        { crouch: 0.5, front: -0.6, back: -0.6 },
        { crouch: 0.8, front: -1, back: -1 },
        { front: 2.8, back: 2.6 },
        { front: 3, back: 2.8, bob: -4 },
        { lean: 0.25, front: H, back: H, reach: 1.6, backReach: 1.4, stride: 1 },
        { lean: 0.1, front: 0.8, back: 0.6 },
      ],
      frameMs: [80, 80, 80, 80, 160, 260],
      loop: false,
      impact: 4,
    },
    hit: {
      poses: [
        { lean: -0.35, front: 0.6, back: -0.6, dx: -2 },
        { lean: -0.45, front: 0.8, back: -0.8, dx: -4 },
        { lean: -0.2, front: 0.4, back: -0.4, dx: -2 },
      ],
      frameMs: [80, 120, 200],
      loop: false,
    },
    block: {
      poses: [
        { lean: -0.05, front: 2.2, back: 2, reach: 0.7, backReach: 0.7 },
        { lean: -0.1, front: 2.3, back: 2.1, reach: 0.7, backReach: 0.7, crouch: 0.2 },
      ],
      frameMs: [100, 250],
      loop: false,
    },
    dodge: {
      poses: [
        { crouch: 0.6, lean: -0.3, dx: -4 },
        { crouch: 0.9, lean: -0.4, dx: -8 },
        { crouch: 0.5, lean: -0.2, dx: -4 },
      ],
      frameMs: [80, 160, 160],
      loop: false,
    },
    knockdown: {
      poses: [
        { fall: 0.3, front: 1 },
        { fall: 0.6, front: 1.4 },
        { fall: 1, front: 2 },
        { fall: 1, front: 1.6 },
      ],
      frameMs: [100, 150, 250, 400],
      loop: false,
    },
    ko: {
      poses: [
        { fall: 0.35, front: 1.2, back: -1 },
        { fall: 0.7, front: 1.8, back: -1.6 },
        { fall: 1, front: 2.4, back: -2 },
        { fall: 1, front: 2.8, back: -2.6 },
      ],
      frameMs: [100, 150, 250, 500],
      loop: false,
    },
    win: {
      poses: [0, -2, -4, -2].map((bob) => ({ bob, front: 2.9, back: -2.9 })),
      frameMs: [200, 200, 200, 200],
      loop: true,
    },
  };
}

const ORDER: readonly AnimationCode[] = [
  'idle',
  'walk',
  'intro',
  'attack_light',
  'attack_heavy',
  'special',
  'hit',
  'block',
  'dodge',
  'knockdown',
  'ko',
  'win',
];

const SHEET_MAX_WIDTH = 1024;

export interface PlaceholderSheet {
  readonly file: string;
  readonly width: number;
  readonly height: number;
  /** RGBA, 8 bits per channel, not premultiplied. */
  readonly data: Uint8Array;
}

export interface Placeholder {
  readonly manifest: SpriteManifest;
  readonly sheets: readonly PlaceholderSheet[];
}

/** Coordinates are snapped to 1/16 px, so the raster does not depend on float noise. */
const q = (v: number): number => Math.round(v * 16) / 16;

class Raster {
  readonly data: Uint8Array;
  readonly width: number;
  readonly height: number;

  constructor(width: number, height: number) {
    this.width = width;
    this.height = height;
    this.data = new Uint8Array(width * height * 4);
  }

  private set(x: number, y: number, c: Rgba): void {
    const i = (y * this.width + x) * 4;
    this.data[i] = c[0];
    this.data[i + 1] = c[1];
    this.data[i + 2] = c[2];
    this.data[i + 3] = c[3];
  }

  /** Fills a convex polygon, clipped to the cell [cx, cx + cw) × [cy, cy + ch). */
  polygon(points: readonly Point[], c: Rgba, clip: readonly number[]): void {
    const [cx = 0, cy = 0, cw = 0, ch = 0] = clip;
    const xs = points.map((p) => p[0]);
    const ys = points.map((p) => p[1]);
    const x0 = Math.max(cx, Math.floor(Math.min(...xs)));
    const x1 = Math.min(cx + cw - 1, Math.ceil(Math.max(...xs)));
    const y0 = Math.max(cy, Math.floor(Math.min(...ys)));
    const y1 = Math.min(cy + ch - 1, Math.ceil(Math.max(...ys)));
    for (let y = y0; y <= y1; y++) {
      for (let x = x0; x <= x1; x++) {
        if (inside(points, x + 0.5, y + 0.5)) this.set(x, y, c);
      }
    }
  }

  circle(center: Point, r: number, c: Rgba, clip: readonly number[]): void {
    const [cx = 0, cy = 0, cw = 0, ch = 0] = clip;
    const x0 = Math.max(cx, Math.floor(center[0] - r));
    const x1 = Math.min(cx + cw - 1, Math.ceil(center[0] + r));
    const y0 = Math.max(cy, Math.floor(center[1] - r));
    const y1 = Math.min(cy + ch - 1, Math.ceil(center[1] + r));
    for (let y = y0; y <= y1; y++) {
      for (let x = x0; x <= x1; x++) {
        const dx = x + 0.5 - center[0];
        const dy = y + 0.5 - center[1];
        if (dx * dx + dy * dy <= r * r) this.set(x, y, c);
      }
    }
  }
}

function inside(points: readonly Point[], x: number, y: number): boolean {
  let sign = 0;
  for (let i = 0; i < points.length; i++) {
    const a = points[i] ?? [0, 0];
    const b = points[(i + 1) % points.length] ?? [0, 0];
    const cross = (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
    if (cross === 0) continue;
    const s = cross > 0 ? 1 : -1;
    if (sign === 0) sign = s;
    else if (s !== sign) return false;
  }
  return true;
}

const add = (p: Point, v: Point, k = 1): Point => [p[0] + v[0] * k, p[1] + v[1] * k];

function thickLine(p: Point, r: Point, width: number): Point[] {
  const dx = r[0] - p[0];
  const dy = r[1] - p[1];
  const len = Math.hypot(dx, dy) || 1;
  const n: Point = [(-dy / len) * (width / 2), (dx / len) * (width / 2)];
  return [add(p, n), add(r, n), add(r, n, -1), add(p, n, -1)];
}

/** Draws one pose in the cell whose top-left corner is (ox, oy). */
function drawPose(
  raster: Raster,
  body: Body,
  palette: Palette,
  pose: Pose,
  ox: number,
  oy: number,
) {
  const lean = pose.lean ?? 0;
  const crouch = pose.crouch ?? 0;
  const stride = pose.stride ?? 0;
  const fall = pose.fall ?? 0;

  // Figure space: origin at the feet, x forward (right), y down.
  const hip: Point = [0, -body.legLen * (1 - 0.35 * crouch)];
  const up: Point = [Math.sin(lean), -Math.cos(lean)];
  const side: Point = [Math.cos(lean), Math.sin(lean)];
  const top = add(hip, up, body.torsoLen);
  const torso: Point[] = [
    add(hip, side, -body.torsoBottom / 2),
    add(hip, side, body.torsoBottom / 2),
    add(top, side, body.torsoTop / 2),
    add(top, side, -body.torsoTop / 2),
  ];
  const headCenter = add(top, up, 2 + body.headSize);
  const shoulder = add(top, up, -3);
  const arm = (angle: number, reach: number): Point[] =>
    thickLine(
      shoulder,
      add(shoulder, [Math.sin(angle), Math.cos(angle)], body.armLen * reach),
      body.armWidth,
    );
  const spread = 5 + 6 * crouch;
  const legs = [
    thickLine(add(hip, [-3, 0]), [-spread + stride * 7, 0], body.legWidth),
    thickLine(add(hip, [3, 0]), [spread - stride * 7, 0], body.legWidth),
  ];

  // Falling backward: rotate around the feet, shrink, and slide right so the body fits.
  const angle = -fall * H;
  const scale = 1 - 0.3 * fall;
  const total = body.legLen + body.torsoLen + 2 + 2 * body.headSize;
  const shift = fall * Math.max(0, total * scale - body.pivotX + 3);
  const lift = fall * (body.torsoTop / 2 + 1);
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  const tx = (p: Point): Point => {
    const x = (p[0] * cos - p[1] * sin) * scale;
    const y = (p[0] * sin + p[1] * cos) * scale;
    return [
      q(ox + body.pivotX + (pose.dx ?? 0) + shift + x),
      q(oy + body.pivotY + (pose.bob ?? 0) - lift + y),
    ];
  };
  const clip = [ox, oy, body.frameW, body.frameH];
  const poly = (points: readonly Point[], c: Rgba) => raster.polygon(points.map(tx), c, clip);

  poly(arm(pose.back ?? -0.2, pose.backReach ?? 1), palette.dark);
  for (const leg of legs) poly(leg, palette.dark);
  poly(torso, palette.body);
  if (body.head === 'circle') {
    raster.circle(tx(headCenter), q(body.headSize * scale), palette.head, clip);
  } else {
    const s = body.headSize / 2;
    poly(
      [
        add(add(headCenter, side, -s), up, -s),
        add(add(headCenter, side, s), up, -s),
        add(add(headCenter, side, s), up, s),
        add(add(headCenter, side, -s), up, s),
      ],
      palette.head,
    );
  }
  poly(arm(pose.front ?? 0.3, pose.reach ?? 1), palette.body);
}

export function drawPlaceholder(id: PlaceholderId): Placeholder {
  const body = BODIES[id];
  const palette = PALETTES[id];
  const specs = animations(id);
  const poseCount = ORDER.reduce((n, code) => n + specs[code].poses.length, 0);
  const cols = Math.floor(SHEET_MAX_WIDTH / body.frameW);
  const rows = Math.ceil(poseCount / cols);
  const width = cols * body.frameW;
  const height = rows * body.frameH;
  const raster = new Raster(width, height);

  const frames: SpriteManifest['frames'][number][] = [];
  const anims: Partial<Record<AnimationCode, SpriteAnimation>> = {};
  for (const code of ORDER) {
    const spec = specs[code];
    const indices: number[] = [];
    for (const pose of spec.poses) {
      const index = frames.length;
      const x = (index % cols) * body.frameW;
      const y = Math.floor(index / cols) * body.frameH;
      drawPose(raster, body, palette, pose, x, y);
      frames.push({
        sheet: 0,
        x,
        y,
        w: body.frameW,
        h: body.frameH,
        pivot_x: body.pivotX,
        pivot_y: body.pivotY,
      });
      indices.push(index);
    }
    anims[code] =
      spec.impact === undefined
        ? { frames: indices, frame_ms: spec.frameMs, loop: spec.loop }
        : { frames: indices, frame_ms: spec.frameMs, loop: spec.loop, impact_frame: spec.impact };
  }

  const manifest: SpriteManifest = {
    format: 'torneo.sprites',
    format_version: 1,
    id,
    pixel_art: true,
    sheets: [{ file: 'sheet-0.png', width, height }],
    frames,
    animations: anims as SpriteManifest['animations'],
  };
  return { manifest, sheets: [{ file: 'sheet-0.png', width, height, data: raster.data }] };
}
