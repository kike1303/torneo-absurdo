import { describe, expect, it } from 'vitest';
import { validateManifest } from '@torneo/renderer/manifest';
import { prepareScene, sceneAt, type PreparedScene } from '@torneo/renderer/scene';
import { validateTimeline, type Timeline } from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import {
  liveTimelineFromEvents,
  timelineFromFightLog,
} from '../src/lib/features/fight/timeline-from-fight-log.js';
import { KO_LOGS, TIME_LOGS, readGoldenLog, type GoldenLogName } from './support/golden-logs.js';

// 010 RF49: for the 7 goldens and every cut `c`, the scene of the prefix is the scene of the whole
// log for t ≤ c − 3500. The prefix goes the way of a live fight: events with `t ≤ c` (RF40), the
// adapter and the partial validator.
const NAMES: readonly GoldenLogName[] = [...KO_LOGS, ...TIME_LOGS];
const ASSETS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;
const MAX_DURATION_MS = 90_000;
const PLAYBACK_DELAY_MS = 3500;
/** The end of the fight is 4000 ms after `ko` or `time_up`, its closing 1500 ms after them. */
const CLOSING_TO_END_MS = 2500;
const GRID_SPAN_MS = 4500;
const GRID_STEP_MS = 100;
/** Every so many cuts, also a coarse sweep of the whole past. */
const SWEEP_EVERY = 10;
const SWEEP_STEP_MS = 250;

const a = validateManifest(dummyA, { expectedId: 'dummy_a' });
const b = validateManifest(dummyB, { expectedId: 'dummy_b' });
if (!a.ok || !b.ok) throw new Error('placeholder manifests do not validate');
const MANIFESTS = new Map([
  ['dummy_a', a.manifest],
  ['dummy_b', b.manifest],
]);

function fullTimeline(name: GoldenLogName): Timeline {
  const result = timelineFromFightLog(readGoldenLog(name), ASSETS);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.timeline;
}

function prefixTimeline(name: GoldenLogName, c: number): Timeline {
  const log = readGoldenLog(name);
  const events = log.events.filter((event) => event.t <= c);
  const result = liveTimelineFromEvents(log, events, ASSETS, MAX_DURATION_MS);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  const checked = validateTimeline(result.timeline, MANIFESTS, { partial: true });
  if (!checked.ok) throw new Error(JSON.stringify(checked.errors));
  return checked.timeline;
}

/** The strongest cut of each prefix (just before the next event) and each event's own `t`. */
function cutsOf(name: GoldenLogName): number[] {
  const times = [...new Set(readGoldenLog(name).events.map((event) => event.t))];
  const cuts = times.flatMap((t, i) => {
    const next = times[i + 1];
    return next === undefined ? [t, t + GRID_SPAN_MS] : [t, next - 1];
  });
  return [...new Set(cuts)];
}

function instants(tMax: number, sweep: boolean): number[] {
  const out = new Set<number>([0, tMax]);
  for (let t = Math.max(0, tMax - GRID_SPAN_MS); t < tMax; t += GRID_STEP_MS) out.add(t);
  if (sweep) for (let t = 0; t < tMax; t += SWEEP_STEP_MS) out.add(t);
  return [...out];
}

const sceneJson = (prepared: PreparedScene, t: number, reducedMotion: boolean) =>
  JSON.stringify(sceneAt(prepared, t, { reducedMotion }));

describe('prefixes of a fight (010 RF49)', () => {
  it.each(NAMES)('%s: same scene as the whole log for t ≤ c − 3500', (name) => {
    const full = prepareScene(fullTimeline(name), MANIFESTS);
    let checks = 0;
    cutsOf(name).forEach((c, index) => {
      const tMax = c - PLAYBACK_DELAY_MS;
      if (tMax < 0) return;
      const prefix = prepareScene(prefixTimeline(name, c), MANIFESTS);
      for (const t of instants(tMax, index % SWEEP_EVERY === 0)) {
        for (const reducedMotion of [false, true]) {
          checks++;
          const got = sceneJson(prefix, t, reducedMotion);
          const want = sceneJson(full, t, reducedMotion);
          // Compared as text first: `expect` on every instant would take minutes.
          if (got !== want) expect({ name, c, t, reducedMotion, got }).toEqual({ want });
        }
      }
    });
    expect(checks).toBeGreaterThan(1000);
  });

  it.each(NAMES)('%s: the margin is not empty, one ms short of 2500 the scene differs', (name) => {
    const timeline = fullTimeline(name);
    const end = timeline.duration_ms;
    const closing = end - CLOSING_TO_END_MS;
    const c = end - 1;
    expect(c - closing).toBe(CLOSING_TO_END_MS - 1);
    const full = prepareScene(timeline, MANIFESTS);
    const prefix = prepareScene(prefixTimeline(name, c), MANIFESTS);
    expect(sceneAt(prefix, closing - 1, { reducedMotion: false })).toEqual(
      sceneAt(full, closing - 1, { reducedMotion: false }),
    );
    expect(sceneAt(prefix, closing, { reducedMotion: false })).not.toEqual(
      sceneAt(full, closing, { reducedMotion: false }),
    );
  });
});
