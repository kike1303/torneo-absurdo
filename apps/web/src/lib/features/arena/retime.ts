// `?pace=<factor>` on /dev/arena (002 RF-B9, plan-b decision 17; endings by time in 005 RF17;
// dynamic minimum and clamp in 006 RF14 to RF16): rescales the fight to try another mean gap
// between strikes. Pure. The validator is injected (006 plan, architect decision 1): this module
// only imports renderer types, and `FightStage` passes `(t) => validateTimeline(t).ok`.
import type { Cue, Timeline } from '@torneo/renderer/timeline';

/** The lowest factor `paceMin` tries (006 RF15). */
export const PACE_FLOOR = 0.5;
export const PACE_MAX = 1.5;
const PACE = /^[0-9](?:\.[0-9]{1,2})?$/;

/** Whether a timeline passes the renderer's rules (without manifests: retiming never needs them). */
export type TimelineCheck = (timeline: Timeline) => boolean;

/** What `?pace` resolves to for one timeline (006 RF16). */
export interface PaceResolution {
  requested: number;
  /** The smallest valid factor, or null when the timeline is already invalid with factor 1. */
  min: number | null;
  effective: number;
  clamped: boolean;
}

/** The factor of `?pace=`, or null when absent or malformed. Only the syntax (0 to 9.99, 006 RF14). */
export function parsePace(raw: string | null): number | null {
  if (raw === null || !PACE.test(raw)) return null;
  return Number(raw);
}

/**
 * The smallest factor, in hundredths from 1 down to `PACE_FLOOR`, such that every factor from it to
 * 1 keeps `timeline` valid; null when factor 1 is already invalid (006 RF15). `k / 100`, never
 * `k * 0.01`, so that `86 / 100 === 0.86` exactly.
 */
export function paceMin(timeline: Timeline, isValid: TimelineCheck): number | null {
  const floor = Math.round(PACE_FLOOR * 100);
  let min: number | null = null;
  for (let k = 100; k >= floor; k--) {
    if (!isValid(retimeTimeline(timeline, k / 100))) break;
    min = k / 100;
  }
  return min;
}

/** Clamps `requested` to `[paceMin, PACE_MAX]`; with no minimum it leaves the timeline alone (006 RF16). */
export function resolvePace(
  requested: number,
  timeline: Timeline,
  isValid: TimelineCheck,
): PaceResolution {
  const min = paceMin(timeline, isValid);
  if (min === null) return { requested, min, effective: 1, clamped: requested !== 1 };
  const effective = Math.min(Math.max(requested, min), PACE_MAX);
  return { requested, min, effective, clamped: effective !== requested };
}

/**
 * Scales every instant from the first strike on by `factor`. A knockdown keeps its offset from
 * the strike before it and its duration. The ending (`ko` or `time_up`) is scaled; its closing
 * (`win` or `draw`) keeps its gap after it, and the tail after the closing (the pose) keeps its
 * length.
 */
export function retimeTimeline(timeline: Timeline, factor: number): Timeline {
  if (factor === 1) return timeline;
  const first = timeline.cues.find((c) => c.type === 'strike')?.at_ms;
  if (first === undefined) return timeline;
  const scale = (at: number) => (at < first ? at : Math.round(first + (at - first) * factor));
  let strike = { from: first, to: first };
  let anchor = { from: first, to: first };
  let closing: { from: number; to: number } | null = null;
  const cues = timeline.cues.map((cue): Cue => {
    switch (cue.type) {
      case 'strike': {
        const to = scale(cue.at_ms);
        strike = { from: cue.at_ms, to };
        return { ...cue, at_ms: to };
      }
      case 'knockdown':
        return { ...cue, at_ms: strike.to + (cue.at_ms - strike.from) };
      case 'ko':
      case 'time_up': {
        const to = scale(cue.at_ms);
        anchor = { from: cue.at_ms, to };
        return { ...cue, at_ms: to };
      }
      case 'win':
      case 'draw': {
        const to = anchor.to + (cue.at_ms - anchor.from);
        closing = { from: cue.at_ms, to };
        return { ...cue, at_ms: to };
      }
      default:
        return cue;
    }
  });
  const end: { from: number; to: number } = closing ?? anchor;
  return { ...timeline, cues, duration_ms: end.to + (timeline.duration_ms - end.from) };
}

/** Mean gap between consecutive strikes, in whole ms (null with fewer than two strikes). */
export function meanStrikeGapMs(timeline: Pick<Timeline, 'cues'>): number | null {
  const at = timeline.cues.filter((c) => c.type === 'strike').map((c) => c.at_ms);
  if (at.length < 2) return null;
  return Math.round(((at.at(-1) ?? 0) - (at[0] ?? 0)) / (at.length - 1));
}
