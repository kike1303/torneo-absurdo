// `?pace=<factor>` on /dev/arena (002 RF-B9, plan-b decision 17): rescales the fight of the
// spike script to try another mean gap between strikes. Pure; the renderer validates the result
// in `load` (a factor that breaks a rule ends in `timeline_invalid`, never forced).
import type { Cue, Timeline } from '@torneo/renderer/timeline';

export const PACE_MIN = 0.8;
export const PACE_MAX = 1.5;
const PACE = /^[0-9](?:\.[0-9]{1,2})?$/;

/** The factor of `?pace=`, or null when absent, malformed or out of [0.8, 1.5]. */
export function parsePace(raw: string | null): number | null {
  if (raw === null || !PACE.test(raw)) return null;
  const value = Number(raw);
  return value >= PACE_MIN && value <= PACE_MAX ? value : null;
}

/**
 * Scales every instant from the first strike on by `factor`. A knockdown keeps its offset from
 * the strike before it and its duration; `win` keeps its gap after `ko`; the tail after `win`
 * (the victory pose) keeps its length.
 */
export function retimeTimeline(timeline: Timeline, factor: number): Timeline {
  if (factor === 1) return timeline;
  const first = timeline.cues.find((c) => c.type === 'strike')?.at_ms;
  if (first === undefined) return timeline;
  const scale = (at: number) => (at < first ? at : Math.round(first + (at - first) * factor));
  let strike = { from: first, to: first };
  let ko = { from: first, to: first };
  let win: { from: number; to: number } | null = null;
  const cues = timeline.cues.map((cue): Cue => {
    switch (cue.type) {
      case 'strike': {
        const to = scale(cue.at_ms);
        strike = { from: cue.at_ms, to };
        return { ...cue, at_ms: to };
      }
      case 'knockdown':
        return { ...cue, at_ms: strike.to + (cue.at_ms - strike.from) };
      case 'ko': {
        const to = scale(cue.at_ms);
        ko = { from: cue.at_ms, to };
        return { ...cue, at_ms: to };
      }
      case 'win': {
        const to = ko.to + (cue.at_ms - ko.from);
        win = { from: cue.at_ms, to };
        return { ...cue, at_ms: to };
      }
      default:
        return cue;
    }
  });
  const end: { from: number; to: number } = win ?? ko;
  return { ...timeline, cues, duration_ms: end.to + (timeline.duration_ms - end.from) };
}

/** Mean gap between consecutive strikes, in whole ms (null with fewer than two strikes). */
export function meanStrikeGapMs(timeline: Pick<Timeline, 'cues'>): number | null {
  const at = timeline.cues.filter((c) => c.type === 'strike').map((c) => c.at_ms);
  if (at.length < 2) return null;
  return Math.round(((at.at(-1) ?? 0) - (at[0] ?? 0)) / (at.length - 1));
}
