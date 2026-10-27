import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { validateManifest } from '@torneo/renderer/manifest';
import { validateTimeline } from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import { timelineFromFightLog } from '../src/lib/features/fight/timeline-from-fight-log.js';
import {
  KO_LOGS,
  UNSUPPORTED_LOGS,
  goldenLogNames,
  readGoldenBytes,
  readGoldenLog,
} from './support/golden-logs.js';

// 002 RF-C2 (plan-c decision 5, step 3): the engine goldens, read in place, through the adapter
// and `validateTimeline` with the placeholder manifests.
const ASSETS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;
const a = validateManifest(dummyA, { expectedId: 'dummy_a' });
const b = validateManifest(dummyB, { expectedId: 'dummy_b' });
if (!a.ok || !b.ok) throw new Error('placeholder manifests do not validate');
const MANIFESTS = new Map([
  ['dummy_a', a.manifest],
  ['dummy_b', b.manifest],
]);
const RESOLUTIONS = new Set(['hit', 'block', 'dodge']);

describe('engine goldens through the adapter', () => {
  it('the log-*.json in the directory are exactly KO_LOGS and UNSUPPORTED_LOGS', () => {
    expect(goldenLogNames()).toEqual([...KO_LOGS, ...UNSUPPORTED_LOGS].sort());
  });

  it.each(KO_LOGS)('%s gives a timeline that v1 accepts', (name) => {
    const log = readGoldenLog(name);
    const result = timelineFromFightLog(log, ASSETS);
    if (!result.ok) throw new Error(JSON.stringify(result.errors));
    const checked = validateTimeline(result.timeline, MANIFESTS);
    expect(checked.ok ? [] : checked.errors).toEqual([]);
    const strikes = result.timeline.cues.filter((c) => c.type === 'strike');
    expect(strikes).toHaveLength(log.events.filter((e) => RESOLUTIONS.has(e.type)).length);
    const end = log.events.at(-1);
    const win = result.timeline.cues.find((c) => c.type === 'win');
    expect(end?.type === 'end' ? end.winner : 'no end').toBe(win?.fighter);
  });

  it.each(UNSUPPORTED_LOGS)('%s gives only adapter.unsupported_ending', (name) => {
    const result = timelineFromFightLog(readGoldenLog(name), ASSETS);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(new Set(result.errors.map((e) => e.code))).toEqual(
      new Set(['adapter.unsupported_ending']),
    );
  });

  it('the arena copy is byte by byte its golden', () => {
    const copy = readFileSync(
      new URL('../src/lib/features/fight/fixtures/log-ko-comeback.json', import.meta.url),
    );
    expect(copy.equals(readGoldenBytes('log-ko-comeback'))).toBe(true);
  });
});
