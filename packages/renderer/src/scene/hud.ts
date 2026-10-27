// HUD: values in `sceneAt` and layout in CSS px of the container (RF-A13, plan-a1
// decisions 4 and 16).
import type { Side } from '../timeline/types.ts';
import type { HpStep, PreparedFighter } from './prepare.ts';
import type { HudFighterScene } from './types.ts';

/** The trail follows the HP `TRAIL_DELAY_MS` late, easing each step over `TRAIL_EASE_MS`. */
export const TRAIL_DELAY_MS = 450;
export const TRAIL_EASE_MS = 350;

function lastStepAtOrBefore(steps: readonly HpStep[], t: number): HpStep | undefined {
  // Steps are few and sorted; a binary search over their times keeps this O(log n).
  let lo = 0;
  let hi = steps.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if ((steps[mid]?.at ?? 0) <= t) lo = mid + 1;
    else hi = mid;
  }
  return steps[lo - 1];
}

export function hpAt(fighter: PreparedFighter, t: number): number {
  return lastStepAtOrBefore(fighter.hp, t)?.after ?? fighter.maxHp;
}

/** HP with every step eased over TRAIL_EASE_MS; never below the shown HP. */
function hpEased(fighter: PreparedFighter, t: number): number {
  const step = lastStepAtOrBefore(fighter.hp, t);
  if (step === undefined) return fighter.maxHp;
  const p = (t - step.at) / TRAIL_EASE_MS;
  if (p >= 1) return step.after;
  return step.before + (step.after - step.before) * p;
}

export function hudFighterAt(fighter: PreparedFighter, t: number): HudFighterScene {
  const hp = hpAt(fighter, t);
  const trail = Math.max(hp, hpEased(fighter, t - TRAIL_DELAY_MS));
  return { hp, trail, maxHp: fighter.maxHp };
}

export interface Viewport {
  /** CSS px of the container. */
  readonly width: number;
  readonly height: number;
}

/** Width of `text` at `fontPx`, measured by the host's text engine (fake in tests). */
export type MeasureText = (text: string, fontPx: number) => number;

export interface Rect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface HudNameLayout {
  readonly text: string;
  /** Left edge for `a`, right edge for `b`. */
  readonly x: number;
  readonly y: number;
  readonly align: 'start' | 'end';
  readonly width: number;
}

export interface HudLayout {
  readonly fontPx: number;
  readonly bars: Readonly<Record<Side, Rect>>;
  readonly names: Readonly<Record<Side, HudNameLayout>>;
  readonly banner: { readonly x: number; readonly y: number; readonly fontPx: number };
}

export const HUD_MIN_FONT_PX = 14;
const HUD_MAX_FONT_PX = 22;
const ELLIPSIS = '…';

/** Cuts `text` by code points and appends an ellipsis until it fits in `maxWidth`. */
export function fitText(
  text: string,
  maxWidth: number,
  fontPx: number,
  measure: MeasureText,
): string {
  if (measure(text, fontPx) <= maxWidth) return text;
  const chars = Array.from(text);
  // Binary search over the number of kept code points.
  let lo = 0;
  let hi = chars.length;
  while (lo < hi) {
    const mid = (lo + hi + 1) >>> 1;
    const candidate = chars.slice(0, mid).join('').trimEnd() + ELLIPSIS;
    if (measure(candidate, fontPx) <= maxWidth) lo = mid;
    else hi = mid - 1;
  }
  return lo === 0 ? ELLIPSIS : chars.slice(0, lo).join('').trimEnd() + ELLIPSIS;
}

export function layoutHud(
  viewport: Viewport,
  measure: MeasureText,
  names: Readonly<Record<Side, string>>,
): HudLayout {
  const { width, height } = viewport;
  const pad = Math.max(8, Math.round(width * 0.03));
  const gap = Math.max(12, Math.round(width * 0.04));
  const fontPx = Math.min(HUD_MAX_FONT_PX, Math.max(HUD_MIN_FONT_PX, Math.round(height * 0.04)));
  const barHeight = Math.max(10, Math.round(height * 0.035));
  const barWidth = Math.max(0, (width - 2 * pad - gap) / 2);
  const nameY = pad + barHeight + Math.round(fontPx * 0.4);

  const bars = {
    a: { x: pad, y: pad, width: barWidth, height: barHeight },
    b: { x: width - pad - barWidth, y: pad, width: barWidth, height: barHeight },
  };
  const nameFor = (side: Side): HudNameLayout => {
    const text = fitText(names[side], barWidth, fontPx, measure);
    return {
      text,
      x: side === 'a' ? pad : width - pad,
      y: nameY,
      align: side === 'a' ? 'start' : 'end',
      width: measure(text, fontPx),
    };
  };
  return {
    fontPx,
    bars,
    names: { a: nameFor('a'), b: nameFor('b') },
    banner: {
      x: width / 2,
      y: height / 2,
      fontPx: Math.max(28, Math.round(height * 0.14)),
    },
  };
}
