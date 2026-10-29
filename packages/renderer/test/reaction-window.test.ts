import dummyA from '../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../placeholders/dummy_b/manifest.json' with { type: 'json' };
import { describe, expect, it } from 'vitest';
import { mulberry32 } from '../src/fx/prng.ts';
import {
  ATTACK_ANIMATIONS,
  REACTION_WINDOW_MS,
  validateManifest,
  windupMs,
  type AnimationCode,
  type SpriteManifest,
} from '../src/manifest/index.ts';
import { prepareScene, sceneAt } from '../src/scene/index.ts';
import {
  MOVES,
  otherSide,
  validateTimeline,
  type Side,
  type StrikeCue,
  type StrikeResult,
  type Timeline,
} from '../src/timeline/index.ts';
import { MANIFESTS } from './support/placeholders.ts';
import { GOLDEN_NAMES, LONG, goldenTimeline, spikeTimeline } from './support/scenes.ts';
import { timeline } from './support/timeline-builder.ts';

// 007 RF-A1: with any valid timeline, nobody receives an impact during their own windup, and their
// next windup starts at least REACTION_WINDOW_MS after the impact received.
const OPTS = { reducedMotion: false };
const ATTACKS: readonly AnimationCode[] = ATTACK_ANIMATIONS;

/** The placeholders with the three windups at exactly 400 ms: the C2 edge with 600 ms gaps. */
function maxManifest(json: unknown, id: string): SpriteManifest {
  const base = structuredClone(json) as Record<string, unknown> & {
    animations: Record<string, { frame_ms: number[] }>;
  };
  base.id = id;
  const a = base.animations;
  const set = (code: string, ms: number[]) => {
    const anim = a[code];
    if (anim) anim.frame_ms = ms;
  };
  set('attack_light', [200, 200, 90, 120]);
  set('attack_heavy', [100, 100, 200, 150, 220]);
  set('special', [100, 100, 100, 100, 160, 260]);
  const result = validateManifest(base, { expectedId: id });
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

const MAX_MANIFESTS: ReadonlyMap<string, SpriteManifest> = new Map([
  ['max_a', maxManifest(dummyA, 'max_a')],
  ['max_b', maxManifest(dummyB, 'max_b')],
]);

interface ManifestSet {
  readonly name: string;
  readonly manifests: ReadonlyMap<string, SpriteManifest>;
  readonly assets: Readonly<Record<Side, string>>;
}

const SETS: readonly ManifestSet[] = [
  { name: 'placeholders', manifests: MANIFESTS, assets: { a: 'dummy_a', b: 'dummy_b' } },
  { name: 'long', ...LONG },
  { name: 'max', manifests: MAX_MANIFESTS, assets: { a: 'max_a', b: 'max_b' } },
];

const GAPS = [600, 600, 600, 600, 601, 700, 1000, 2500];
const RESULTS: readonly StrikeResult[] = ['hit', 'crit', 'block', 'dodge'];

function pick<T>(rng: () => number, items: readonly T[]): T {
  return items[Math.floor(rng() * items.length)] as T;
}

/** A random valid timeline (valid by construction: the builder validates and never filters). */
function generated(seed: number, set: ManifestSet): Timeline {
  const rng = mulberry32(seed);
  const b = timeline({ maxHp: 1_000_000, manifests: set.manifests, assets: set.assets });
  let at = 2000 + Math.floor(rng() * 3) * 100;
  for (let i = 0; i < 20; i++) {
    const actor: Side = rng() < 0.5 ? 'a' : 'b';
    const result = pick(rng, RESULTS);
    b.strike(at, actor, pick(rng, MOVES), result, result === 'dodge' ? 0 : 10);
    let next = at + pick(rng, GAPS);
    if (result === 'crit' && rng() < 0.3) {
      const lag = rng() < 0.5 ? 0 : 100;
      const duration = 800 + Math.floor(rng() * 8) * 100;
      b.knockdown(at + lag, otherSide(actor), duration);
      next = Math.max(next, at + lag + duration + 1);
    }
    at = next;
  }
  return b.finish(at, rng() < 0.5 ? 'a' : 'b');
}

/** A fixed timeline with its fighters pointed at another manifest set. */
function retargeted(source: Timeline, set: ManifestSet): Timeline {
  const json = structuredClone(source) as { fighters: Record<Side, { asset: string }> };
  json.fighters.a.asset = set.assets.a;
  json.fighters.b.asset = set.assets.b;
  const result = validateTimeline(json, set.manifests);
  if (!result.ok) throw new Error(JSON.stringify(result.errors.slice(0, 5)));
  return result.timeline;
}

interface Violation {
  readonly impact: number;
  readonly target: Side;
  readonly tau: number;
  readonly animation: AnimationCode;
}

function check(tl: Timeline, set: ManifestSet): { violations: Violation[]; early: number[] } {
  const prepared = prepareScene(tl, set.manifests);
  const violations: Violation[] = [];
  const early: number[] = [];
  const strikes = tl.cues.filter((c): c is StrikeCue => c.type === 'strike');
  const samples = [];
  for (let d = 0; d < REACTION_WINDOW_MS; d += 20) samples.push(d);
  samples.push(REACTION_WINDOW_MS - 1);
  for (const s of strikes) {
    const target = otherSide(s.actor);
    for (const d of samples) {
      const tau = s.at_ms + d;
      const animation = sceneAt(prepared, tau, OPTS).fighters[target].animation;
      if (ATTACKS.includes(animation)) {
        violations.push({ impact: s.at_ms, target, tau, animation });
      }
    }
    for (const cue of prepared.cues) {
      for (const seg of cue.segments) {
        if (seg.side !== target || seg.kind !== 'windup') continue;
        if (seg.start > s.at_ms && seg.start < s.at_ms + REACTION_WINDOW_MS) early.push(seg.start);
      }
    }
  }
  return { violations, early };
}

describe('the choreography keeps the reaction window (007 RF-A1)', () => {
  it('the MAX manifests have the three windups at the cap', () => {
    for (const m of MAX_MANIFESTS.values()) {
      for (const move of ATTACK_ANIMATIONS) {
        expect(windupMs(m.animations[move])).toBe(400);
      }
    }
  });

  describe.each(SETS)('with the $name manifests', (set) => {
    it('holds on 200 generated timelines', () => {
      for (let seed = 1; seed <= 200; seed++) {
        const tl = generated(seed, set);
        expect(validateTimeline(tl, set.manifests).ok).toBe(true);
        expect({ seed, ...check(tl, set) }).toEqual({ seed, violations: [], early: [] });
      }
    });

    it('holds on the spike script and the engine goldens', () => {
      const fixed: [string, Timeline][] = [
        ['spike', spikeTimeline()],
        ...GOLDEN_NAMES.map((n): [string, Timeline] => [n, goldenTimeline(n)]),
      ];
      for (const [name, source] of fixed) {
        const tl = retargeted(source, set);
        expect({ name, ...check(tl, set) }).toEqual({ name, violations: [], early: [] });
      }
    });
  });
});
