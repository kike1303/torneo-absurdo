import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import spike from '../fixtures/spike.timeline.json' with { type: 'json' };
import { validateTimeline } from '../src/timeline/index.ts';
import { applyOps, loadFixtures } from './support/patch.ts';
import { MANIFESTS } from './support/placeholders.ts';

// 005 plan decision 1 (RF2): the full result of validateTimeline for every v1 input, recorded with
// the code from before timeline v2. The snapshot must never change: a v1 validates exactly as
// before (same `ok`, codes, paths and data).
const { valid, rules } = loadFixtures(
  fileURLToPath(new URL('./fixtures/timeline/', import.meta.url)),
);

type Base = typeof valid & { cues: unknown[] };

function v1Ending(closing: Record<string, unknown>): unknown {
  const doc = structuredClone(valid) as Base;
  // Drop the finishing strike, `ko` and `win`, then end by time.
  doc.cues = doc.cues.slice(0, 6);
  doc.cues.push({ at_ms: 8000, type: 'time_up' }, { at_ms: 9500, ...closing });
  return doc;
}

function inputs(): [string, unknown, boolean][] {
  const out: [string, unknown, boolean][] = [
    ['_valid', valid, false],
    ['_valid+manifests', valid, true],
    ['spike', spike, true],
    ['v1 time_up and win', v1Ending({ type: 'win', fighter: 'a' }), false],
    ['v1 time_up and draw', v1Ending({ type: 'draw' }), false],
    ['not object', [], false],
    ['missing version', { duration_ms: 1 }, false],
  ];
  for (const rule of rules) {
    // `unsupported_version` moved from 2 to 3 when v2 became valid (spec concern 4); its result
    // does not change.
    out.push([rule.name, applyOps(valid, rule.ops), rule.manifests === true]);
  }
  return out.sort(([a], [b]) => (a < b ? -1 : a > b ? 1 : 0));
}

describe('validateTimeline v1 characterization', () => {
  it('gives exactly the recorded result for every v1 input', async () => {
    const results: Record<string, unknown> = {};
    for (const [name, json, withManifests] of inputs()) {
      const result = validateTimeline(json, withManifests ? MANIFESTS : undefined);
      results[name] = result.ok
        ? {
            ok: true,
            timeline_version: result.timeline.timeline_version,
            duration_ms: result.timeline.duration_ms,
            cues: result.timeline.cues.length,
          }
        : result;
    }
    await expect(`${JSON.stringify(results, null, 2)}\n`).toMatchFileSnapshot(
      './__snapshots__/timeline-v1.snap.json',
    );
  });
});
