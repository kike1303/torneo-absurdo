// validateGuide (007 RF-A3, RF-A4): the guide layer over a manifest already validated by
// validateManifest, with the decoded pixels of its sheets. Pure and deterministic; it never
// decodes PNG. Codes and JSON Pointers of the manifest, never text (ADR-0007).
import { ErrorSink, StopValidation, pointer } from '../manifest/input.ts';
import { windupMs } from '../manifest/resolve.ts';
import {
  ATTACK_ANIMATIONS,
  LIMITS,
  type AnimationCode,
  type AttackAnimation,
  type SpriteAnimation,
  type SpriteFrame,
  type SpriteManifest,
} from '../manifest/types.ts';
import { CONTACT_PX } from '../scene/choreography.ts';
import { GUIDE_CODES as G } from './codes.ts';
import { GUIDE_LIMITS } from './limits.ts';
import type { RulesetTiming } from './ruleset.ts';
import { guideTable, type GuideRow } from './table.ts';

/** The decoded pixels of one sheet: 8-bit RGBA, row-major. */
export interface RgbaImage {
  readonly width: number;
  readonly height: number;
  readonly data: Uint8Array;
}

export type GuideErrorCode = `guide.${string}`;

export interface GuideError {
  readonly code: GuideErrorCode;
  readonly path: string;
  readonly data?: Readonly<Record<string, string | number>>;
}

export type GuideResult =
  { readonly ok: true } | { readonly ok: false; readonly errors: readonly GuideError[] };

type Sink = ErrorSink<GuideErrorCode>;

/** The opaque box of a frame that measures the cell and has pixels (cell coordinates). */
interface Box {
  readonly top: number;
  readonly bottom: number;
  readonly right: number;
}

const PIVOT_X = GUIDE_LIMITS.cellPx / 2;
const PIVOT_Y = GUIDE_LIMITS.cellPx - GUIDE_LIMITS.groundMarginPx;
const ATTACKS: readonly AnimationCode[] = ATTACK_ANIMATIONS;

/** The caller guarantees the pixels match `sheets[]` (phase B checks the IHDR first). */
function checkSheets(manifest: SpriteManifest, sheets: readonly RgbaImage[]): void {
  if (sheets.length !== manifest.sheets.length) throw new RangeError('guide.sheets_mismatch');
  manifest.sheets.forEach((declared, i) => {
    const image = sheets[i];
    if (
      image === undefined ||
      image.width !== declared.width ||
      image.height !== declared.height ||
      image.data.length !== image.width * image.height * 4
    ) {
      throw new RangeError('guide.sheets_mismatch');
    }
  });
}

function alphaAt(image: RgbaImage, frame: SpriteFrame, x: number, y: number): number {
  return image.data[((frame.y + y) * image.width + frame.x + x) * 4 + 3] ?? 0;
}

/** One pass over the cell: its opaque box, or null when empty; and whether it touches the edge. */
function scan(image: RgbaImage, frame: SpriteFrame): { box: Box | null; clipped: boolean } {
  const last = GUIDE_LIMITS.cellPx - 1;
  let top = -1;
  let bottom = -1;
  let right = -1;
  let clipped = false;
  for (let y = 0; y <= last; y++) {
    for (let x = 0; x <= last; x++) {
      if (alphaAt(image, frame, x, y) === 0) continue;
      if (top < 0) top = y;
      bottom = y;
      if (x > right) right = x;
      if (x === 0 || y === 0 || x === last || y === last) clipped = true;
    }
  }
  return { box: top < 0 ? null : { top, bottom, right }, clipped };
}

/** Same size and same bytes: compares contents instead of indices (`idle_distinct`). */
function samePixels(sheets: readonly RgbaImage[], a: SpriteFrame, b: SpriteFrame): boolean {
  if (a.w !== b.w || a.h !== b.h) return false;
  const ia = sheets[a.sheet];
  const ib = sheets[b.sheet];
  if (ia === undefined || ib === undefined) return false;
  for (let y = 0; y < a.h; y++) {
    const sa = ((a.y + y) * ia.width + a.x) * 4;
    const sb = ((b.y + y) * ib.width + b.x) * 4;
    for (let i = 0; i < a.w * 4; i++) {
      if (ia.data[sa + i] !== ib.data[sb + i]) return false;
    }
  }
  return true;
}

function sum(values: readonly number[], from = 0, to = values.length): number {
  let total = 0;
  for (let i = from; i < to; i++) total += values[i] ?? 0;
  return total;
}

function outside(value: number, min: number, max: number): boolean {
  return value < min || value > max;
}

function checkFrames(
  sink: Sink,
  manifest: SpriteManifest,
  sheets: readonly RgbaImage[],
): Map<number, Box> {
  const boxes = new Map<number, Box>();
  const cell = GUIDE_LIMITS.cellPx;
  manifest.frames.forEach((frame, i) => {
    const path = pointer('frames', i);
    if (frame.w !== cell || frame.h !== cell) {
      sink.add(G.cellSize, path, { w: frame.w, h: frame.h });
      return;
    }
    if (frame.pivot_x !== PIVOT_X || frame.pivot_y !== PIVOT_Y) {
      sink.add(G.pivotPosition, path, { pivot_x: frame.pivot_x, pivot_y: frame.pivot_y });
    }
    const image = sheets[frame.sheet];
    if (image === undefined) return;
    const { box, clipped } = scan(image, frame);
    if (box === null) {
      sink.add(G.frameEmpty, path);
      return;
    }
    if (clipped) sink.add(G.frameClipped, path);
    boxes.set(i, box);
  });
  return boxes;
}

function checkTiming(sink: Sink, code: AnimationCode, anim: SpriteAnimation, row: GuideRow): void {
  const base = ['animations', code] as const;
  const count = anim.frames.length;
  if (outside(count, row.frames.min, row.frames.max)) {
    sink.add(G.framesCount, pointer(...base, 'frames'), {
      count,
      min: row.frames.min,
      max: row.frames.max,
    });
  }
  anim.frame_ms.forEach((ms, k) => {
    if (outside(ms, GUIDE_LIMITS.frameMsMin, GUIDE_LIMITS.frameMsMax)) {
      sink.add(G.frameMsRange, pointer(...base, 'frame_ms', k), { ms });
    }
  });
  const frameMax = row.frame_max_ms;
  if (frameMax !== undefined) {
    anim.frame_ms.forEach((ms, k) => {
      if (ms > frameMax) sink.add(G.idleFrameLong, pointer(...base, 'frame_ms', k), { ms });
    });
  }
  const total = sum(anim.frame_ms);
  for (const [range, rule] of [
    [row.cycle_ms, G.cycleRange],
    [row.duration_ms, G.durationRange],
  ] as const) {
    if (range !== undefined && outside(total, range.min, range.max)) {
      sink.add(rule, pointer(...base, 'frame_ms'), {
        total_ms: total,
        min: range.min,
        max: range.max,
      });
    }
  }
  const impact = anim.impact_frame;
  const windup = row.windup_ms;
  if (impact !== undefined && windup !== undefined) {
    const impactPath = pointer(...base, 'impact_frame');
    const w = windupMs(anim);
    if (w < windup.min) sink.add(G.windupShort, impactPath, { windup_ms: w, min: windup.min });
    if (w > windup.max) {
      sink.add(
        G.windupLong,
        impactPath,
        windup.ruleset_version === undefined
          ? { windup_ms: w, cap_ms: windup.max }
          : { windup_ms: w, cap_ms: windup.max, ruleset_version: windup.ruleset_version },
      );
    }
    const impactMin = row.impact_frame_min ?? 0;
    if (impact < impactMin) {
      sink.add(G.anticipation, impactPath, { impact_frame: impact, min: impactMin });
    }
    const hold = anim.frame_ms[impact] ?? 0;
    const holdMin = row.impact_hold_min_ms ?? 0;
    if (hold < holdMin) {
      sink.add(G.impactHold, pointer(...base, 'frame_ms', impact), { ms: hold, min: holdMin });
    }
    const recovery = sum(anim.frame_ms, impact);
    const recoveryMin = row.recovery_min_ms ?? 0;
    if (recovery < recoveryMin) {
      sink.add(G.recoveryShort, pointer(...base, 'frame_ms'), {
        recovery_ms: recovery,
        min: recoveryMin,
      });
    }
  }
  const firstMax = row.first_frame_max_ms;
  const first = anim.frame_ms[0];
  if (firstMax !== undefined && first !== undefined && first > firstMax) {
    sink.add(G.reactionSlow, pointer(...base, 'frame_ms', 0), { ms: first, max: firstMax });
  }
}

function checkIdlePixels(sink: Sink, anim: SpriteAnimation, boxes: ReadonlyMap<number, Box>): void {
  const feetMin = PIVOT_Y - 1 - GUIDE_LIMITS.feetTolerancePx;
  const seen = new Set<number>();
  anim.frames.forEach((f, k) => {
    if (seen.has(f)) return;
    seen.add(f);
    const box = boxes.get(f);
    if (box !== undefined && outside(box.bottom, feetMin, PIVOT_Y - 1)) {
      sink.add(G.feetOffPivot, pointer('animations', 'idle', 'frames', k), { row: box.bottom });
    }
  });
  const first = anim.frames[0];
  const box = first === undefined ? undefined : boxes.get(first);
  if (box !== undefined) {
    const height = box.bottom - box.top + 1;
    if (outside(height, GUIDE_LIMITS.heightMinPx, GUIDE_LIMITS.heightMaxPx)) {
      sink.add(G.heightRange, pointer('animations', 'idle', 'frames', 0), { height });
    }
  }
}

function checkReach(
  sink: Sink,
  code: AnimationCode,
  anim: SpriteAnimation,
  boxes: ReadonlyMap<number, Box>,
): void {
  const impact = anim.impact_frame;
  if (impact === undefined) return;
  const frame = anim.frames[impact];
  const box = frame === undefined ? undefined : boxes.get(frame);
  if (box === undefined) return;
  const reach = box.right - PIVOT_X;
  if (reach < CONTACT_PX / 2) {
    sink.add(G.impactShort, pointer('animations', code, 'frames', impact), { reach });
  }
}

function idleDistinct(
  sink: Sink,
  manifest: SpriteManifest,
  sheets: readonly RgbaImage[],
  row: GuideRow,
): void {
  const min = row.distinct_min;
  if (min === undefined) return;
  const distinct: SpriteFrame[] = [];
  for (const f of new Set(manifest.animations.idle.frames)) {
    const frame = manifest.frames[f];
    if (frame !== undefined && !distinct.some((d) => samePixels(sheets, d, frame))) {
      distinct.push(frame);
    }
  }
  if (distinct.length < min) {
    sink.add(G.idleDistinct, pointer('animations', 'idle', 'frames'), { count: distinct.length });
  }
}

function windupOrder(sink: Sink, manifest: SpriteManifest): void {
  const w = (move: AttackAnimation) => windupMs(manifest.animations[move]);
  const data = {
    light_ms: w('attack_light'),
    heavy_ms: w('attack_heavy'),
    special_ms: w('special'),
    min_step: GUIDE_LIMITS.windupStepMs,
  };
  if (data.light_ms + data.min_step > data.heavy_ms) {
    sink.add(G.windupOrder, pointer('animations', 'attack_heavy', 'impact_frame'), data);
  }
  if (data.heavy_ms > data.special_ms) {
    sink.add(G.windupOrder, pointer('animations', 'special', 'impact_frame'), data);
  }
}

/**
 * The guide over a manifest validated by validateManifest, against every ruleset version. Throws
 * `RangeError('guide.sheets_mismatch')` when `sheets` does not match `manifest.sheets` (a caller
 * error, plan-a decision 10).
 */
export function validateGuide(
  manifest: SpriteManifest,
  sheets: readonly RgbaImage[],
  rulesets: readonly RulesetTiming[],
): GuideResult {
  if (rulesets.length === 0) {
    return { ok: false, errors: [{ code: G.noRulesets, path: '' }] };
  }
  checkSheets(manifest, sheets);
  const table = guideTable(rulesets);
  const sink: Sink = new ErrorSink(LIMITS.maxErrors, G.tooManyErrors);
  try {
    if (manifest.scale !== undefined) sink.add(G.scaleForbidden, pointer('scale'));
    const boxes = checkFrames(sink, manifest, sheets);
    for (const row of table) {
      const anim: SpriteAnimation | undefined = manifest.animations[row.animation];
      if (anim === undefined) continue;
      checkTiming(sink, row.animation, anim, row);
      if (row.animation === 'idle') {
        idleDistinct(sink, manifest, sheets, row);
        checkIdlePixels(sink, anim, boxes);
      }
      if (ATTACKS.includes(row.animation)) checkReach(sink, row.animation, anim, boxes);
    }
    windupOrder(sink, manifest);
  } catch (error) {
    if (!(error instanceof StopValidation)) throw error;
  }
  return sink.count === 0 ? { ok: true } : { ok: false, errors: sink.errors };
}
