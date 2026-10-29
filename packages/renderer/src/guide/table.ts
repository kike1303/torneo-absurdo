// The effective ranges of the guide, per animation (007 RF-A4, RF-A6): the hand-picked ranges of
// GUIDE_LIMITS with the derived bounds already resolved. Codes and numbers only, never text.
// validateGuide reads its ranges from here, so the guide of docs/ (phase B) and the validation
// never diverge.
import {
  LIMITS,
  OPTIONAL_ANIMATIONS,
  REACTION_WINDOW_MS,
  REQUIRED_ANIMATIONS,
  type AnimationCode,
  type AttackAnimation,
} from '../manifest/types.ts';
import { TIMELINE_LIMITS } from '../timeline/types.ts';
import { GUIDE_LIMITS } from './limits.ts';
import type { RulesetTiming } from './ruleset.ts';

export interface GuideRange {
  readonly min: number;
  readonly max: number;
}

export interface WindupRange extends GuideRange {
  /** Present only when a ruleset version is the strict limit (the lowest one on a tie). */
  readonly ruleset_version?: number;
}

export interface GuideRow {
  readonly animation: AnimationCode;
  readonly required: boolean;
  readonly frames: GuideRange;
  /** idle */
  readonly distinct_min?: number;
  /** idle */
  readonly frame_max_ms?: number;
  /** idle, walk */
  readonly cycle_ms?: GuideRange;
  /** intro, hit, block, dodge, knockdown, ko (min 0 when there is only a cap) */
  readonly duration_ms?: GuideRange;
  /** attacks */
  readonly windup_ms?: WindupRange;
  readonly impact_frame_min?: number;
  readonly impact_hold_min_ms?: number;
  readonly recovery_min_ms?: number;
  /** hit, block, dodge */
  readonly first_frame_max_ms?: number;
}

const RULESET_WINDUP = Object.freeze({
  attack_light: 'windup_strike_ms',
  attack_heavy: 'windup_heavy_ms',
  special: 'windup_special_ms',
} as const satisfies Record<AttackAnimation, keyof RulesetTiming>);

function requireRulesets(rulesets: readonly RulesetTiming[]): void {
  if (rulesets.length === 0) throw new RangeError('guide.no_rulesets');
}

/** The lowest `windup_<move>_ms` over the versions, and the lowest version that has it. */
function rulesetMin(
  move: AttackAnimation,
  rulesets: readonly RulesetTiming[],
): { readonly ms: number; readonly version: number } {
  requireRulesets(rulesets);
  const key = RULESET_WINDUP[move];
  let best: { ms: number; version: number } | undefined;
  for (const r of rulesets) {
    const ms = r[key];
    if (
      best === undefined ||
      ms < best.ms ||
      (ms === best.ms && r.ruleset_version < best.version)
    ) {
      best = { ms, version: r.ruleset_version };
    }
  }
  // requireRulesets guarantees one iteration.
  return best as { ms: number; version: number };
}

/** `min(LIMITS.windupMaxMs, min_v windup_<move>_ms(v))` (RF-A4). */
export function windupCap(move: AttackAnimation, rulesets: readonly RulesetTiming[]): number {
  return Math.min(LIMITS.windupMaxMs, rulesetMin(move, rulesets).ms);
}

/**
 * The windup maximum of the guide for a move: the structural cap (and the 250 ms of
 * `attack_light`) or the ruleset, which only carries its version when it is strictly lower
 * (plan-a decision 11).
 */
function windupBound(
  move: AttackAnimation,
  rulesets: readonly RulesetTiming[],
): { readonly max: number; readonly ruleset_version?: number } {
  const structural =
    move === 'attack_light'
      ? Math.min(LIMITS.windupMaxMs, GUIDE_LIMITS.animations.attack_light.windupMaxMs)
      : LIMITS.windupMaxMs;
  const ruleset = rulesetMin(move, rulesets);
  return ruleset.ms < structural
    ? { max: ruleset.ms, ruleset_version: ruleset.version }
    : { max: structural };
}

function attackRow(
  move: AttackAnimation,
  limits: {
    readonly framesMin: number;
    readonly framesMax: number;
    readonly windupMinMs: number;
    readonly impactFrameMin: number;
    readonly impactHoldMinMs: number;
  },
  rulesets: readonly RulesetTiming[],
): Omit<GuideRow, 'required'> {
  return {
    animation: move,
    frames: { min: limits.framesMin, max: limits.framesMax },
    windup_ms: { min: limits.windupMinMs, ...windupBound(move, rulesets) },
    impact_frame_min: limits.impactFrameMin,
    impact_hold_min_ms: limits.impactHoldMinMs,
    recovery_min_ms: REACTION_WINDOW_MS,
  };
}

function reactionRow(
  animation: 'hit' | 'block' | 'dodge',
  limits: { readonly framesMin: number; readonly framesMax: number; readonly totalMaxMs: number },
): Omit<GuideRow, 'required'> {
  return {
    animation,
    frames: { min: limits.framesMin, max: limits.framesMax },
    duration_ms: { min: REACTION_WINDOW_MS, max: limits.totalMaxMs },
    first_frame_max_ms: REACTION_WINDOW_MS - GUIDE_LIMITS.keyPoseMinMs,
  };
}

function rowOf(
  code: AnimationCode,
  rulesets: readonly RulesetTiming[],
): Omit<GuideRow, 'required'> {
  const a = GUIDE_LIMITS.animations;
  switch (code) {
    case 'idle':
      return {
        animation: code,
        frames: { min: a.idle.framesMin, max: a.idle.framesMax },
        distinct_min: a.idle.distinctMin,
        frame_max_ms: a.idle.frameMaxMs,
        cycle_ms: { min: a.idle.cycleMinMs, max: a.idle.cycleMaxMs },
      };
    case 'walk':
      return {
        animation: code,
        frames: { min: a.walk.framesMin, max: a.walk.framesMax },
        cycle_ms: { min: a.walk.cycleMinMs, max: a.walk.cycleMaxMs },
      };
    case 'intro':
      return {
        animation: code,
        frames: { min: a.intro.framesMin, max: a.intro.framesMax },
        duration_ms: { min: 0, max: TIMELINE_LIMITS.introGapMs - LIMITS.windupMaxMs },
      };
    case 'attack_light':
    case 'attack_heavy':
    case 'special':
      return attackRow(code, a[code], rulesets);
    case 'hit':
    case 'block':
    case 'dodge':
      return reactionRow(code, a[code]);
    case 'knockdown': {
      let cap = Number.POSITIVE_INFINITY;
      for (const r of rulesets) cap = Math.min(cap, r.knockdown_ms);
      return {
        animation: code,
        frames: { min: a.knockdown.framesMin, max: a.knockdown.framesMax },
        duration_ms: { min: 0, max: cap },
      };
    }
    case 'ko':
      return {
        animation: code,
        frames: { min: a.ko.framesMin, max: a.ko.framesMax },
        duration_ms: { min: 0, max: TIMELINE_LIMITS.winGapMs },
      };
    case 'win':
      return { animation: code, frames: { min: a.win.framesMin, max: a.win.framesMax } };
  }
}

const REQUIRED: readonly AnimationCode[] = REQUIRED_ANIMATIONS;

/** One row per animation, required ones first, in the canonical order of the manifest. */
export function guideTable(rulesets: readonly RulesetTiming[]): readonly GuideRow[] {
  requireRulesets(rulesets);
  return [...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS].map((code) => {
    const { animation, ...rest } = rowOf(code, rulesets);
    return { animation, required: REQUIRED.includes(code), ...rest };
  });
}
