import { readFileSync, readdirSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import { validateManifest } from '@torneo/renderer/manifest';
import { validateTimeline } from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import { ENGINE_LOGS } from '../src/lib/features/fight/engine-logs.js';
import { timelineFromFightLog } from '../src/lib/features/fight/timeline-from-fight-log.js';
import {
  KO_LOGS,
  TIME_LOGS,
  goldenLogNames,
  readGoldenBytes,
  readGoldenLog,
  type GoldenLogName,
} from './support/golden-logs.js';

// 002 RF-C2 (plan-c decision 5, step 3) and 005 RF4, RF14 and RF15: the engine goldens, read in
// place, through the adapter and `validateTimeline` with the placeholder manifests.
const ASSETS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;
const a = validateManifest(dummyA, { expectedId: 'dummy_a' });
const b = validateManifest(dummyB, { expectedId: 'dummy_b' });
if (!a.ok || !b.ok) throw new Error('placeholder manifests do not validate');
const MANIFESTS = new Map([
  ['dummy_a', a.manifest],
  ['dummy_b', b.manifest],
]);
const RESOLUTIONS = new Set(['hit', 'block', 'dodge']);
const FIXTURES = new URL('../src/lib/features/fight/fixtures/', import.meta.url);

function adapt(name: GoldenLogName) {
  const log = readGoldenLog(name);
  const result = timelineFromFightLog(log, ASSETS);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  const checked = validateTimeline(result.timeline, MANIFESTS);
  expect(checked.ok ? [] : checked.errors).toEqual([]);
  const strikes = result.timeline.cues.filter((c) => c.type === 'strike');
  expect(strikes).toHaveLength(log.events.filter((e) => RESOLUTIONS.has(e.type)).length);
  const end = log.events.at(-1);
  return { log, timeline: result.timeline, end: end?.type === 'end' ? end : null };
}

describe('engine goldens through the adapter', () => {
  it('the log-*.json in the directory are exactly KO_LOGS and TIME_LOGS', () => {
    expect(goldenLogNames()).toEqual([...KO_LOGS, ...TIME_LOGS].sort());
  });

  it.each(KO_LOGS)('%s gives a v2 timeline that is also valid as v1 (RF4)', (name) => {
    const { timeline, end } = adapt(name);
    expect(timeline.timeline_version).toBe(2);
    const win = timeline.cues.find((c) => c.type === 'win');
    expect(end?.winner).toBe(win?.fighter);
    const asV1 = validateTimeline({ ...timeline, timeline_version: 1 }, MANIFESTS);
    expect(asV1.ok ? { ...asV1.timeline, timeline_version: 2 } : asV1.errors).toEqual(timeline);
  });

  it.each(TIME_LOGS)('%s gives a v2 timeline with time_up and the right closing', (name) => {
    const { timeline, end } = adapt(name);
    const timeUp = timeline.cues.find((c) => c.type === 'time_up');
    expect(timeUp).toEqual({ at_ms: 86000, type: 'time_up' });
    expect(timeline.cues.some((c) => c.type === 'ko')).toBe(false);
    expect(timeline.duration_ms).toBe(90000);
    const closing = timeline.cues.at(-1);
    if (end?.reason === 'draw') {
      expect(end.winner).toBeNull();
      expect(closing).toEqual({ at_ms: 87500, type: 'draw' });
      expect(timeline.cues.some((c) => c.type === 'win')).toBe(false);
    } else {
      expect(end?.reason).toBe('decision');
      expect(closing).toEqual({ at_ms: 87500, type: 'win', fighter: end?.winner });
    }
    expect(validateTimeline({ ...timeline, timeline_version: 1 }, MANIFESTS).ok).toBe(false);
  });

  it('the winners of the decisions are a and b', () => {
    const winner = (name: GoldenLogName) => adapt(name).end?.winner;
    expect([winner('log-decision-a'), winner('log-decision-b')]).toEqual(['a', 'b']);
  });
});

describe('the arena copies (005 RF15)', () => {
  const copies = readdirSync(FIXTURES).filter((f) => f.endsWith('.json'));

  it.each(copies)('%s is byte by byte its golden', (file) => {
    const name = file.replace(/\.json$/, '') as GoldenLogName;
    const copy = readFileSync(new URL(file, FIXTURES));
    expect(copy.equals(readGoldenBytes(name))).toBe(true);
  });

  it('the copies are exactly the keys of ENGINE_LOGS, with the log- prefix', () => {
    const keys = Object.keys(ENGINE_LOGS).map((k) => `log-${k}.json`);
    expect([...copies].sort()).toEqual(keys.sort());
    expect(readdirSync(FIXTURES).sort()).toEqual(keys.sort());
  });
});
