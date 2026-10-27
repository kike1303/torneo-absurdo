// Shared scenes for the scene tests: the spike script, long-animation manifests and a
// synthetic 1 000-cue timeline.
import spike from '../../fixtures/spike.timeline.json' with { type: 'json' };
import dummyA from '../../placeholders/dummy_a/manifest.json' with { type: 'json' };
import dummyB from '../../placeholders/dummy_b/manifest.json' with { type: 'json' };
import { mulberry32 } from '../../src/fx/prng.ts';
import { validateManifest, type SpriteManifest } from '../../src/manifest/index.ts';
import { prepareScene, type PreparedScene } from '../../src/scene/index.ts';
import { validateTimeline, type Side, type Timeline } from '../../src/timeline/index.ts';
import { MANIFESTS } from './placeholders.ts';
import { timeline } from './timeline-builder.ts';

export function spikeTimeline(): Timeline {
  const result = validateTimeline(spike, MANIFESTS);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.timeline;
}

export const spikeScene = (): PreparedScene => prepareScene(spikeTimeline(), MANIFESTS);

/** Times from `from` to `to` (inclusive) every `step` ms. */
export function times(from: number, to: number, step: number): number[] {
  const out: number[] = [];
  for (let t = from; t <= to; t += step) out.push(t);
  return out;
}

/** `count` times spread over [0, duration] (deterministic). */
export function spread(duration: number, count: number): number[] {
  return Array.from({ length: count }, (_, i) => Math.round((i * duration) / (count - 1)));
}

/** Fisher-Yates with the package PRNG (never Math.random). */
export function shuffled<T>(items: readonly T[], seed: number): T[] {
  const rng = mulberry32(seed);
  const out = [...items];
  for (let i = out.length - 1; i > 0; i--) {
    const j = Math.floor(rng() * (i + 1));
    [out[i], out[j]] = [out[j] as T, out[i] as T];
  }
  return out;
}

/**
 * Placeholders with long reactions and recoveries (up to the format limits), to force the
 * overlaps of the precedence rules.
 */
function longManifest(json: unknown, id: string): SpriteManifest {
  const base = structuredClone(json) as Record<string, unknown> & {
    animations: Record<string, { frames: number[]; frame_ms: number[] }>;
  };
  base.id = id;
  const a = base.animations;
  const stretch = (code: string, ms: number[]) => {
    const anim = a[code];
    if (anim) anim.frame_ms = ms.slice(0, anim.frames.length);
  };
  stretch('attack_light', [60, 60, 1000, 1000]);
  stretch('attack_heavy', [100, 100, 100, 1200, 1200]);
  stretch('hit', [80, 1200, 1200]);
  stretch('block', [1200, 1200]);
  stretch('dodge', [80, 1200, 1200]);
  const result = validateManifest(base, { expectedId: id });
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

export const LONG_MANIFESTS: ReadonlyMap<string, SpriteManifest> = new Map([
  ['long_a', longManifest(dummyA, 'long_a')],
  ['long_b', longManifest(dummyB, 'long_b')],
]);
export const LONG = { manifests: LONG_MANIFESTS, assets: { a: 'long_a', b: 'long_b' } } as const;

/** A valid timeline of exactly 1 000 cues: 991 strikes, 6 knockdowns, intro, KO and win. */
export function syntheticTimeline(): Timeline {
  const rng = mulberry32(1000);
  const b = timeline({ maxHp: 1_000_000 });
  let at = 2000;
  let knockdowns = 0;
  for (let i = 0; i < 990; i++) {
    const actor: Side = rng() < 0.5 ? 'a' : 'b';
    const knock = knockdowns < 6 && i % 150 === 149;
    b.strike(at, actor, 'attack_light', knock ? 'crit' : 'hit', 1);
    if (knock) {
      b.knockdown(at, actor === 'a' ? 'b' : 'a', 800);
      knockdowns++;
      at += 900;
    } else {
      at += 600 + Math.floor(rng() * 4) * 100;
    }
  }
  return b.finish(at, 'a');
}
