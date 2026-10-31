import dummyA from '../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../placeholders/dummy_b/manifest.json' with { type: 'json' };
import { describe, expect, it } from 'vitest';
import { mulberry32 } from '../src/fx/prng.ts';
import { LIMITS, validateManifest, type SpriteManifest } from '../src/manifest/index.ts';
import {
  MAX_ACTION_MS,
  MAX_WINDUP_MS,
  RETURN_MS,
  prepareScene,
  prepareSceneFrom,
  type PreparedCue,
} from '../src/scene/index.ts';
import {
  MOVES,
  TIMELINE_LIMITS,
  validateTimeline,
  type Move,
  type Side,
  type StrikeResult,
  type Timeline,
} from '../src/timeline/index.ts';
import { TimelineBuilder } from './support/timeline-builder.ts';

// 010 plan-c decision 8: what `prepareSceneFrom` reuses, with manifests at the limits of the
// format. The placeholders have windups up to 320 ms and actions up to 1050 ms, so with them any
// limit passes, however lax: only these data can tell a correct limit from a wrong one.
const FIGHT_END_MS = 80_000;
const SEEDS = [1, 2, 3, 4, 5, 6];
const WIDE_GAP_MS = { min: 3300, max: 4300 };
const CLOSING_GAP_MS = { min: 1100, max: 1300 };
const STRIKE_GAP = TIMELINE_LIMITS.strikeGapMs;
const KNOCKDOWN = { min: TIMELINE_LIMITS.knockdownMinMs, max: TIMELINE_LIMITS.knockdownMaxMs };

/** `total` ms split evenly over `frames` frames, the remainder in the first one. */
function split(frames: number, total: number): number[] {
  const each = Math.floor(total / frames);
  return Array.from({ length: frames }, (_, i) => (i === 0 ? total - each * (frames - 1) : each));
}

/** A placeholder with every attack and reaction as long as the format allows. */
function atTheLimits(json: unknown, id: string): SpriteManifest {
  const base = structuredClone(json) as Record<string, unknown> & {
    animations: Record<string, { frames: number[]; frame_ms: number[]; impact_frame?: number }>;
  };
  base.id = id;
  for (const code of [...MOVES, 'hit', 'block', 'dodge']) {
    const animation = base.animations[code];
    if (animation === undefined) continue;
    const count = animation.frames.length;
    const impact = animation.impact_frame;
    animation.frame_ms =
      impact === undefined
        ? split(count, LIMITS.nonLoopingTotalMaxMs)
        : [
            ...split(impact, LIMITS.windupMaxMs),
            ...split(count - impact, LIMITS.nonLoopingTotalMaxMs - LIMITS.windupMaxMs),
          ];
  }
  const result = validateManifest(base, { expectedId: id });
  if (!result.ok) throw new Error(JSON.stringify(result.errors.slice(0, 5)));
  return result.manifest;
}

const MANIFESTS: ReadonlyMap<string, SpriteManifest> = new Map([
  ['limit_a', atTheLimits(dummyA, 'limit_a')],
  ['limit_b', atTheLimits(dummyB, 'limit_b')],
]);

const RESULTS: readonly StrikeResult[] = ['hit', 'crit', 'block', 'dodge'];

/** Strikes 600 ms apart, gaps near what `H` reaches and knockdowns of up to 3000 ms. */
function generated(seed: number): Timeline {
  const rng = mulberry32(seed);
  const pick = <T>(list: readonly T[]): T => list[Math.floor(rng() * list.length)] as T;
  const between = (min: number, max: number) => min + Math.floor(rng() * (max - min + 1));
  const builder = new TimelineBuilder({
    maxHp: 1_000_000,
    manifests: MANIFESTS,
    assets: { a: 'limit_a', b: 'limit_b' },
    version: 2,
  });
  let at = TIMELINE_LIMITS.introGapMs + between(0, 500);
  while (at < FIGHT_END_MS) {
    const actor: Side = pick(['a', 'b']);
    const move: Move = pick(MOVES);
    const result = pick(RESULTS);
    builder.strike(at, actor, move, result, 1);
    const shape = rng();
    if ((result === 'hit' || result === 'crit') && shape < 0.2) {
      const duration = between(KNOCKDOWN.min, KNOCKDOWN.max);
      builder.knockdown(at, actor === 'a' ? 'b' : 'a', duration);
      at += duration + between(1, 200);
    } else if (shape < 0.45) {
      at += between(WIDE_GAP_MS.min, WIDE_GAP_MS.max);
    } else {
      at += STRIKE_GAP + between(0, 100) * (shape < 0.75 ? 0 : 1);
    }
  }
  // A closing whose fighter is still in the action of the last strike: the only way for a cue that
  // is not the last one to change later, since every strike ends what both fighters were doing.
  builder.strike(at, pick(['a', 'b']), 'attack_heavy', 'hit', 1);
  const ending = seed % 3 === 0 ? null : pick<Side>(['a', 'b']);
  return builder.finishByTime(at + between(CLOSING_GAP_MS.min, CLOSING_GAP_MS.max), ending);
}

/** The timeline after the events up to `c`: no closing and a provisional duration. */
function cut(timeline: Timeline, c: number): Timeline {
  const prefix = {
    ...timeline,
    duration_ms: 90_000,
    cues: timeline.cues.filter((q) => q.at_ms <= c && q.type !== 'win' && q.type !== 'draw'),
  };
  const result = validateTimeline(prefix, MANIFESTS, { partial: true });
  if (!result.ok) throw new Error(JSON.stringify(result.errors.slice(0, 5)));
  return result.timeline;
}

const timesOf = (timeline: Timeline): number[] => [
  ...new Set(
    timeline.cues.filter((q) => q.type !== 'draw' && q.type !== 'win').map((q) => q.at_ms),
  ),
];

/** The cue of the same index in the scene of a prefix and in the one of the whole timeline. */
const sameCue = (x: PreparedCue | undefined, y: PreparedCue | undefined) =>
  JSON.stringify(x) === JSON.stringify(y);

describe('prepareSceneFrom at the limits of the format', () => {
  it('the data reach the limits', () => {
    expect(MAX_WINDUP_MS).toBe(LIMITS.windupMaxMs);
    const timing = prepareScene(generated(1), MANIFESTS).fighters.a.timing;
    expect(timing.attack_heavy.windup).toBe(LIMITS.windupMaxMs);
    expect(timing.attack_heavy.total).toBe(LIMITS.nonLoopingTotalMaxMs);
    expect(timing.hit.total).toBe(LIMITS.nonLoopingTotalMaxMs);
  });

  describe.each(SEEDS)('seed %i', (seed) => {
    const timeline = generated(seed);
    const times = timesOf(timeline);

    it.each([1, 3])('is the same as prepareScene, %i cuts at a time', (batch) => {
      let previous = prepareScene(cut(timeline, 0), MANIFESTS);
      for (let i = batch; i < times.length; i += batch) {
        const next = cut(timeline, times[i] as number);
        previous = prepareSceneFrom(previous, next);
        expect(previous).toStrictEqual(prepareScene(next, MANIFESTS));
      }
      previous = prepareSceneFrom(previous, timeline);
      expect(previous).toStrictEqual(prepareScene(timeline, MANIFESTS));
    });

    it('stays frozen, as prepareScene', () => {
      const first = prepareScene(cut(timeline, times[5] as number), MANIFESTS);
      const next = prepareSceneFrom(first, cut(timeline, times[20] as number));
      expect(Object.isFrozen(next)).toBe(true);
      expect(Object.isFrozen(next.cues)).toBe(true);
      expect(Object.isFrozen(next.cues.at(-1))).toBe(true);
      expect(Object.isFrozen(next.cues.at(-1)?.segments)).toBe(true);
    });
  });

  it('reuses the cues before the limit, and only those', () => {
    const timeline = generated(1);
    const times = timesOf(timeline);
    const previous = prepareScene(cut(timeline, times[40] as number), MANIFESTS);
    const next = prepareSceneFrom(previous, cut(timeline, times[41] as number));
    const last = previous.cueTimes.at(-1) as number;
    const limit = last - MAX_WINDUP_MS - RETURN_MS - MAX_ACTION_MS - RETURN_MS;
    previous.cues.forEach((cue, i) => {
      expect(next.cues[i] === cue, `cue ${i}`).toBe(cue.cue.at_ms < limit);
    });
    expect(next.cues[0]).toBe(previous.cues[0]);
    expect(next.fighters.a.timing).toBe(previous.fighters.a.timing);
  });

  // Negative control: if nothing here differed, the test above would pass with any limit. A cue
  // well behind the last one still changes when the next ones arrive, so a laxer limit would reuse
  // a stale cue.
  it('with these data, a cue behind the last one can still change', () => {
    const margins = new Set<number>();
    for (const seed of SEEDS) {
      const timeline = generated(seed);
      const full = prepareScene(timeline, MANIFESTS);
      for (const c of timesOf(timeline)) {
        const prefix = prepareScene(cut(timeline, c), MANIFESTS);
        const last = prefix.cueTimes.at(-1) as number;
        prefix.cues.forEach((cue, i) => {
          if (!sameCue(cue, full.cues[i])) margins.add(last - cue.cue.at_ms);
        });
      }
    }
    // `at_ms < last` is the laxest limit, and `at_ms + 1000 < last` a closer one: both would fail.
    expect(Math.max(...margins)).toBeGreaterThan(1000);
  });

  it('nothing beyond the limit ever differs: the limit is enough, not only in the cuts above', () => {
    const limit = MAX_WINDUP_MS + RETURN_MS + MAX_ACTION_MS + RETURN_MS;
    for (const seed of SEEDS) {
      const timeline = generated(seed);
      const full = prepareScene(timeline, MANIFESTS);
      for (const c of timesOf(timeline)) {
        const prefix = prepareScene(cut(timeline, c), MANIFESTS);
        const last = prefix.cueTimes.at(-1) as number;
        prefix.cues.forEach((cue, i) => {
          if (cue.cue.at_ms < last - limit) expect(sameCue(cue, full.cues[i])).toBe(true);
        });
      }
    }
  });
});
