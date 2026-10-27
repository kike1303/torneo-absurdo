// prepareScene: one pass per `load` that builds immutable indices, so `sceneAt` only does
// binary searches and looks at a bounded window of cues (plan-a1 decision 2).
import { CHANNEL, hashSeed } from '../fx/hash.ts';
import { admitFlashes } from '../fx/flash.ts';
import { resolveAnimation } from '../manifest/resolve.ts';
import {
  OPTIONAL_ANIMATIONS,
  REQUIRED_ANIMATIONS,
  type AnimationCode,
  type SpriteManifest,
} from '../manifest/types.ts';
import type { ManifestsById } from '../timeline/validate.ts';
import { otherSide, type Cue, type Side, type Timeline } from '../timeline/types.ts';
import {
  MAX_ACTION_MS,
  MAX_WINDUP_MS,
  RETURN_MS,
  buildChoreography,
  type AnimationTiming,
  type Hold,
  type Impact,
  type Segment,
  type TimingTable,
} from './choreography.ts';
import { MARK_X } from './types.ts';

/** How far back `cueWindow` looks: the longest action plus the return to the mark. */
export const LOOKBACK_MS = MAX_ACTION_MS + RETURN_MS;
/** How far ahead it looks: the longest windup (RF-A11). */
export const LOOKAHEAD_MS = MAX_WINDUP_MS;
/**
 * Upper bound of `cueWindow(prepared, t).length` for any valid timeline: with ≥ 600 ms between
 * strikes, the 3.7 s window holds at most 7 strikes, plus their knockdowns, the intro, the KO and
 * the win (plan-a1 step 7).
 */
export const K = 16;

export type ImpactKind = 'hit' | 'crit' | 'special' | 'block' | 'dodge';

export interface PreparedStrike {
  readonly impactKind: ImpactKind;
  readonly crit: boolean;
  readonly flash: boolean;
  readonly attackerX: number;
  readonly defenderX: number;
  readonly seeds: { readonly particles: number; readonly shake: number; readonly numbers: number };
}

export interface PreparedCue {
  readonly index: number;
  readonly cue: Cue;
  readonly segments: readonly Segment[];
  readonly strike: PreparedStrike | null;
  readonly shakeSeed: number;
}

export interface HpStep {
  readonly at: number;
  readonly before: number;
  readonly after: number;
}

export interface PreparedFighter {
  readonly side: Side;
  readonly asset: string;
  readonly maxHp: number;
  readonly scale: number;
  readonly timing: TimingTable;
  readonly hold: Hold | null;
  /** HP changes of this fighter (as a target), sorted by time. */
  readonly hp: readonly HpStep[];
}

export interface PreparedScene {
  readonly durationMs: number;
  readonly fighters: Readonly<Record<Side, PreparedFighter>>;
  readonly cues: readonly PreparedCue[];
  /** `at_ms` of every cue, for binary searches. */
  readonly cueTimes: readonly number[];
  readonly firstStrikeAt: number | null;
  readonly ko: { readonly at: number; readonly fighter: Side } | null;
  readonly win: { readonly at: number; readonly fighter: Side } | null;
}

const ALL_ANIMATIONS: readonly AnimationCode[] = [...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS];

function timingTable(manifest: SpriteManifest): TimingTable {
  const table: Partial<Record<AnimationCode, AnimationTiming>> = {};
  for (const code of ALL_ANIMATIONS) {
    const { code: played, animation } = resolveAnimation(manifest, code);
    const ends: number[] = [];
    let sum = 0;
    let windup = 0;
    animation.frame_ms.forEach((ms, i) => {
      if (animation.impact_frame !== undefined && i < animation.impact_frame) windup += ms;
      sum += ms;
      ends.push(sum);
    });
    table[code] = {
      code: played,
      // Copied: deepFreeze must never freeze the caller's manifest.
      frames: [...animation.frames],
      ends,
      total: sum,
      loop: animation.loop,
      windup,
    };
  }
  return table as TimingTable;
}

function deepFreeze<T>(value: T): T {
  if (typeof value === 'object' && value !== null && !Object.isFrozen(value)) {
    Object.freeze(value);
    for (const key of Object.keys(value)) {
      deepFreeze((value as Record<string, unknown>)[key]);
    }
  }
  return value;
}

/** Index of the first element ≥ `x` in a sorted array. */
export function lowerBound(sorted: readonly number[], x: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if ((sorted[mid] ?? 0) < x) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** Index of the first element > `x` in a sorted array. */
export function upperBound(sorted: readonly number[], x: number): number {
  let lo = 0;
  let hi = sorted.length;
  while (lo < hi) {
    const mid = (lo + hi) >>> 1;
    if ((sorted[mid] ?? 0) <= x) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/**
 * The only cues `sceneAt` reads: those with `at_ms` in [t − LOOKBACK_MS, t + LOOKAHEAD_MS].
 * Every segment active at `t` belongs to one of them (see `Segment.owner`).
 */
export function cueWindow(prepared: PreparedScene, t: number): readonly PreparedCue[] {
  const lo = lowerBound(prepared.cueTimes, t - LOOKBACK_MS);
  const hi = upperBound(prepared.cueTimes, t + LOOKAHEAD_MS);
  return prepared.cues.slice(lo, hi);
}

/**
 * Prepares a validated timeline and its manifests (by asset id) for `sceneAt`. The result is
 * deeply frozen, always: the core cannot tell development from production (plan-a1 decision 2).
 */
export function prepareScene(timeline: Timeline, manifests: ManifestsById): PreparedScene {
  const manifestOf = (side: Side): SpriteManifest => {
    const manifest = manifests.get(timeline.fighters[side].asset);
    if (manifest === undefined) throw new Error('timeline.unknown_asset');
    return manifest;
  };
  const manifestsBySide = { a: manifestOf('a'), b: manifestOf('b') };
  const timings = { a: timingTable(manifestsBySide.a), b: timingTable(manifestsBySide.b) };
  const choreography = buildChoreography(timeline, timings);

  const hp: Record<Side, HpStep[]> = { a: [], b: [] };
  const current: Record<Side, number> = {
    a: timeline.fighters.a.max_hp,
    b: timeline.fighters.b.max_hp,
  };

  const flashCandidates: number[] = [];
  const flashIndex: number[] = [];
  timeline.cues.forEach((cue, index) => {
    if (cue.type !== 'strike') return;
    const target = otherSide(cue.actor);
    hp[target].push({ at: cue.at_ms, before: current[target], after: cue.target_hp });
    current[target] = cue.target_hp;
    if (cue.result === 'hit' || cue.result === 'crit') {
      flashCandidates.push(cue.at_ms);
      flashIndex.push(index);
    }
  });
  const admitted = new Set<number>();
  admitFlashes(flashCandidates).forEach((ok, i) => {
    const index = flashIndex[i];
    if (ok && index !== undefined) admitted.add(index);
  });

  let firstStrikeAt: number | null = null;
  let ko: PreparedScene['ko'] = null;
  let win: PreparedScene['win'] = null;

  const cues: PreparedCue[] = timeline.cues.map((cue, index) => {
    let strike: PreparedStrike | null = null;
    if (cue.type === 'strike') {
      if (firstStrikeAt === null) firstStrikeAt = cue.at_ms;
      const impact: Impact = choreography.impacts[index] ?? {
        attackerX: MARK_X[cue.actor],
        defenderX: MARK_X[otherSide(cue.actor)],
      };
      const impactKind: ImpactKind =
        cue.result === 'dodge'
          ? 'dodge'
          : cue.result === 'block'
            ? 'block'
            : cue.move === 'special'
              ? 'special'
              : cue.result;
      strike = {
        impactKind,
        crit: cue.result === 'crit',
        flash: admitted.has(index),
        attackerX: impact.attackerX,
        defenderX: impact.defenderX,
        seeds: {
          particles: hashSeed(index, CHANNEL.PARTICLES),
          shake: hashSeed(index, CHANNEL.SHAKE),
          numbers: hashSeed(index, CHANNEL.NUMBERS),
        },
      };
    } else if (cue.type === 'ko' && ko === null) {
      ko = { at: cue.at_ms, fighter: cue.fighter };
    } else if (cue.type === 'win' && win === null) {
      win = { at: cue.at_ms, fighter: cue.fighter };
    }
    return {
      index,
      cue: { ...cue },
      segments: choreography.segments[index] ?? [],
      strike,
      shakeSeed: hashSeed(index, CHANNEL.SHAKE),
    };
  });

  const fighter = (side: Side): PreparedFighter => ({
    side,
    asset: timeline.fighters[side].asset,
    maxHp: timeline.fighters[side].max_hp,
    scale: manifestsBySide[side].scale ?? 1,
    timing: timings[side],
    hold: choreography.holds[side],
    hp: hp[side],
  });

  const prepared: PreparedScene = {
    durationMs: timeline.duration_ms,
    fighters: { a: fighter('a'), b: fighter('b') },
    cues,
    cueTimes: timeline.cues.map((cue) => cue.at_ms),
    firstStrikeAt,
    ko,
    win,
  };
  return deepFreeze(prepared);
}
