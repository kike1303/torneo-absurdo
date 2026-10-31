// The cost of each `extend` with 200 cues (010 RNF "CPU", plan-c decision 8), one call every 25 ms
// so the garbage collector works as it does with real batches and not as in a tight loop. Run:
// pnpm --filter @torneo/renderer bench:prepare. Node is only half of the approximation: the
// threshold of `prepareSceneFrom` is p50 ≤ 0.67 ms, and the other half is `extend_ms` in Chromium.
import { readFileSync } from 'node:fs';
import { validateManifest, type SpriteManifest } from '../src/manifest/index.ts';
import { prepareScene, prepareSceneFrom } from '../src/scene/index.ts';
import { validateTimeline, type Cue, type Side, type Timeline } from '../src/timeline/index.ts';

const CUES = 200;
const SAMPLES = 400;
const PACE_MS = 25;
const MAX_HP = 1_000_000;

function manifest(id: string): SpriteManifest {
  const url = new URL(`../placeholders/${id}/manifest.json`, import.meta.url);
  const result = validateManifest(JSON.parse(readFileSync(url, 'utf8')) as unknown, {
    expectedId: id,
  });
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

function timelineOf(count: number): Timeline {
  const cues: Cue[] = [{ at_ms: 0, type: 'intro' }];
  const hp: Record<Side, number> = { a: MAX_HP, b: MAX_HP };
  for (let i = 1; i < count; i++) {
    const actor: Side = i % 2 === 0 ? 'a' : 'b';
    const target: Side = actor === 'a' ? 'b' : 'a';
    hp[target] -= 1;
    cues.push({
      at_ms: 2000 + i * 700,
      type: 'strike',
      actor,
      move: i % 3 === 0 ? 'special' : 'attack_light',
      result: i % 5 === 0 ? 'crit' : 'hit',
      damage: 1,
      target_hp: hp[target],
    });
  }
  return {
    timeline_version: 2,
    duration_ms: 3_000_000,
    fighters: {
      a: { asset: 'dummy_a', max_hp: MAX_HP },
      b: { asset: 'dummy_b', max_hp: MAX_HP },
    },
    cues,
  };
}

const manifests = new Map([
  ['dummy_a', manifest('dummy_a')],
  ['dummy_b', manifest('dummy_b')],
]);
const timeline = timelineOf(CUES);
const previous = prepareScene(timelineOf(CUES - 1), manifests);

async function measure(name: string, run: () => void): Promise<void> {
  const samples: number[] = [];
  for (let i = 0; i < SAMPLES; i++) {
    const start = process.hrtime.bigint();
    run();
    samples.push(Number(process.hrtime.bigint() - start) / 1e6);
    await new Promise((resolve) => setTimeout(resolve, PACE_MS));
  }
  samples.sort((p, q) => p - q);
  const at = (q: number) => (samples[Math.floor(q * (SAMPLES - 1))] ?? 0).toFixed(2);
  console.log(`${name}: p50 ${at(0.5)} ms, p95 ${at(0.95)} ms, max ${at(1)} ms`);
}

await measure('prepareScene', () => prepareScene(timeline, manifests));
await measure('prepareSceneFrom', () => prepareSceneFrom(previous, timeline));
await measure('validateTimeline partial', () =>
  validateTimeline(timeline, manifests, { partial: true }),
);
