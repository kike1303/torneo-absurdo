import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';
import { validateManifest, type SpriteManifest } from '@torneo/renderer/manifest';
import { prepareScene, sceneAt } from '@torneo/renderer/scene';
import { runImport } from '../src/import.ts';
import { copyExportRef, goldenRoot } from './support/root.ts';
import { cleanupTmp } from './support/tmp.ts';

afterAll(cleanupTmp);

// 007 RF-B8: the renderer plays ref_a against dummy_b with the spike script and with the engine
// golden timelines, without errors, every 16 ms of the fight. The JSON files are read as bytes
// (no JSON import across packages); they are valid timelines of the renderer's own tests.
type Timeline = Parameters<typeof prepareScene>[0];

const RENDERER = fileURLToPath(new URL('../../../packages/renderer/', import.meta.url));
const readJson = (rel: string): unknown =>
  JSON.parse(readFileSync(path.join(RENDERER, ...rel.split('/')), 'utf8'));

function manifestOf(json: unknown, id: string): SpriteManifest {
  const result = validateManifest(json, { expectedId: id });
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.manifest;
}

function importedRef(): SpriteManifest {
  const root = goldenRoot();
  const outDir = path.join(root, 'content', '_local', 'sprites', 'ref_a');
  const result = runImport({ root, inDir: copyExportRef(), outDir });
  if (result.status !== 0) throw new Error(JSON.stringify(result.issues));
  return manifestOf(JSON.parse(readFileSync(path.join(outDir, 'manifest.json'), 'utf8')), 'ref_a');
}

const REF = importedRef();
const DUMMY_B = manifestOf(readJson('placeholders/dummy_b/manifest.json'), 'dummy_b');
const MANIFESTS = new Map([
  ['ref_a', REF],
  ['dummy_b', DUMMY_B],
]);

/** The timeline with side a pointed at ref_a and side b at dummy_b. */
function retarget(json: unknown): Timeline {
  const t = structuredClone(json) as { fighters: Record<'a' | 'b', { asset: string }> };
  t.fighters.a.asset = 'ref_a';
  t.fighters.b.asset = 'dummy_b';
  return t as unknown as Timeline;
}

const TIMELINES = [
  'fixtures/spike.timeline.json',
  'test/fixtures/golden/decision-a.timeline.json',
  'test/fixtures/golden/decision-b.timeline.json',
  'test/fixtures/golden/draw.timeline.json',
];

describe('ref_a plays against dummy_b', () => {
  it.each(TIMELINES)('%s', (file) => {
    const timeline = retarget(readJson(file));
    const prepared = prepareScene(timeline, MANIFESTS);
    const bad: string[] = [];
    let steps = 0;
    for (let t = 0; t <= prepared.durationMs; t += 16) {
      const scene = sceneAt(prepared, t, { reducedMotion: false });
      for (const side of ['a', 'b'] as const) {
        const f = scene.fighters[side];
        const manifest = side === 'a' ? REF : DUMMY_B;
        if (!Number.isInteger(f.frame) || f.frame < 0 || f.frame >= manifest.frames.length) {
          bad.push(`${side}@${t}:${f.frame}`);
        }
      }
      steps++;
    }
    expect(bad).toEqual([]);
    expect(steps).toBeGreaterThan(1000);
  });
});
