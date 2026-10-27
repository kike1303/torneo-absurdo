// Exhaustive walk behind the bound of `cueWindow` (plan-a2 decision 16a). Pure and without
// randomness: it enumerates every sequence of minimal steps that fits in the window.
//
// Steps:
// - `S`: a strike alone; the next strike comes 600 ms later (the minimum gap);
// - `K`: a strike with its knockdown at the same `at_ms`; the next strike comes 801 ms later
//   (the knockdown lasts ≥ 800 ms and no strike may land in `(at, at + duration]`);
// - `F`: the closing strike, with `ko` at the same `at_ms` and `win` 1 500 ms later.
// The intro is an optional prefix: with it, the first strike comes 2 000 ms after it; without
// it, the sequence starts far from the intro.
//
// Larger gaps than the minimum can only push cues out of a window, so minimal steps suffice.
import { LOOKAHEAD_MS, LOOKBACK_MS, cueWindow, prepareScene } from '../../src/scene/index.ts';
import type { Side, Timeline } from '../../src/timeline/index.ts';
import { MANIFESTS } from './placeholders.ts';
import { timeline } from './timeline-builder.ts';

export type Step = 'S' | 'K' | 'F';

export const WINDOW_MS = LOOKBACK_MS + LOOKAHEAD_MS;
const GAP: Readonly<Record<'S' | 'K', number>> = { S: 600, K: 801 };
const INTRO_GAP = 2000;
const FAR = 20_000;

/** Every sequence of `S`/`K` steps (optionally closed by `F`) whose strikes span ≤ WINDOW_MS. */
export function sequences(): Step[][] {
  const out: Step[][] = [];
  const grow = (steps: Step[], span: number): void => {
    // `span` is the time from the first strike to the strike the next step would add.
    if (steps.length > 0) out.push([...steps]);
    if (span > WINDOW_MS) return;
    out.push([...steps, 'F']);
    for (const step of ['S', 'K'] as const) grow([...steps, step], span + GAP[step]);
  };
  grow([], 0);
  return out;
}

export function build(steps: readonly Step[], intro: boolean): Timeline {
  const b = timeline({ maxHp: 1_000_000 });
  let at = intro ? INTRO_GAP : FAR;
  let actor: Side = 'a';
  for (const step of steps) {
    if (step === 'F') return b.finish(at, actor);
    const target: Side = actor === 'a' ? 'b' : 'a';
    b.strike(at, actor, 'attack_light', 'hit', 1);
    if (step === 'K') b.knockdown(at, target, 800);
    at += GAP[step];
    actor = target;
  }
  return b.finish(at + FAR, 'a');
}

/** The largest `cueWindow` of a timeline: the count only peaks with a cue on a window edge. */
export function maxWindow(tl: Timeline): number {
  const prepared = prepareScene(tl, MANIFESTS);
  let max = 0;
  for (const at of prepared.cueTimes) {
    for (const t of [at + LOOKBACK_MS, at - LOOKAHEAD_MS]) {
      max = Math.max(max, cueWindow(prepared, t).length);
    }
  }
  return max;
}
