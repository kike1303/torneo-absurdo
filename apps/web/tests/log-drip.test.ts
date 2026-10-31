import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { validateManifest } from '@torneo/renderer/manifest';
import { validateTimeline, type Timeline } from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import {
  DRIP_TICK_MS,
  MAX_DURATION_MS,
  PLAYBACK_DELAY_MS,
  durationStats,
  openLogDrip,
  releasedCount,
} from '../src/lib/features/arena/log-drip.js';
import { timelineFromFightLog } from '../src/lib/features/fight/timeline-from-fight-log.js';
import { KO_LOGS, TIME_LOGS, readGoldenLog } from './support/golden-logs.js';

// 010 RF52: the goteo of /dev/arena, with a fake clock and fake timers.
const ASSETS = { a: 'dummy_a', b: 'dummy_b' } as const;
const FIGHTERS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;
const LOG = readGoldenLog('log-ko-knockdown');
const END_T = (LOG.events.at(-1) as { t: number }).t;
const FULL = (() => {
  const result = timelineFromFightLog(LOG, FIGHTERS);
  if (!result.ok) throw new Error(JSON.stringify(result.errors));
  return result.timeline;
})();
const a = validateManifest(dummyA, { expectedId: 'dummy_a' });
const b = validateManifest(dummyB, { expectedId: 'dummy_b' });
if (!a.ok || !b.ok) throw new Error('placeholder manifests do not validate');
const MANIFESTS = new Map([
  ['dummy_a', a.manifest],
  ['dummy_b', b.manifest],
]);

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
});

function open(startElapsedMs = 0) {
  let now = 1000;
  const session = openLogDrip(LOG, ASSETS, { startElapsedMs, now: () => now });
  const pushed: Timeline[] = [];
  const advance = (ms: number) => {
    for (let left = ms; left > 0; left -= DRIP_TICK_MS) {
      now += Math.min(DRIP_TICK_MS, left);
      vi.advanceTimersByTime(Math.min(DRIP_TICK_MS, left));
    }
  };
  return {
    session,
    pushed,
    advance,
    skip: (ms: number) => {
      now += ms;
    },
  };
}

describe('releasedCount', () => {
  it('gives the events with t ≤ elapsed, so a same t never splits', () => {
    expect(releasedCount(LOG.events, -1)).toBe(0);
    expect(releasedCount(LOG.events, 0)).toBe(1);
    expect(releasedCount(LOG.events, END_T - 1)).toBe(LOG.events.length - 1);
    expect(releasedCount(LOG.events, END_T)).toBe(LOG.events.length);
    for (const event of LOG.events) {
      const count = releasedCount(LOG.events, event.t);
      expect(LOG.events[count]?.t ?? Infinity).toBeGreaterThan(event.t);
    }
  });

  it('the maximum of the arena covers every golden', () => {
    for (const name of [...KO_LOGS, ...TIME_LOGS]) {
      const log = readGoldenLog(name);
      expect((log.events.at(-1) as { t: number }).t).toBeLessThanOrEqual(MAX_DURATION_MS);
    }
  });
});

describe('openLogDrip', () => {
  it('opens with the intro and the clock one delay behind the server', () => {
    const { session, advance } = open();
    expect(session.timeline.cues).toEqual([{ at_ms: 0, type: 'intro' }]);
    expect(session.timeline.duration_ms).toBe(MAX_DURATION_MS);
    expect(session.clock()).toBe(-PLAYBACK_DELAY_MS);
    advance(1000);
    expect(session.clock()).toBe(1000 - PLAYBACK_DELAY_MS);
  });

  it('no event goes out before its t, and each timeline is valid and extends the last', () => {
    const { session, pushed, advance } = open();
    let elapsed = 0;
    const seen: number[] = [];
    session.subscribe((timeline) => {
      pushed.push(timeline);
      const newest = timeline.cues.at(-1)?.at_ms ?? 0;
      if (timeline.duration_ms === MAX_DURATION_MS) expect(newest).toBeLessThanOrEqual(elapsed);
      seen.push(timeline.cues.length);
      expect(validateTimeline(timeline, MANIFESTS, { partial: true }).ok).toBe(true);
    });
    while (elapsed < END_T + DRIP_TICK_MS) {
      elapsed += DRIP_TICK_MS;
      advance(DRIP_TICK_MS);
    }
    expect(pushed.length).toBeGreaterThan(5);
    expect(seen).toEqual([...seen].sort((p, q) => p - q));
  });

  it('the last timeline is the one of the whole log', () => {
    const { session, pushed, advance } = open();
    session.subscribe((timeline) => pushed.push(timeline));
    advance(END_T + DRIP_TICK_MS);
    expect(pushed.at(-1)).toEqual(FULL);
  });

  it('publishes only when there are new events, and stops after end', () => {
    const { session, pushed, advance } = open();
    session.subscribe((timeline) => pushed.push(timeline));
    advance(END_T + DRIP_TICK_MS);
    const count = pushed.length;
    expect(vi.getTimerCount()).toBe(0);
    advance(10_000);
    expect(pushed.length).toBe(count);
  });

  it('comes in late with what is already out, and the clock in its place', () => {
    const { session } = open(30_000);
    expect(session.clock()).toBe(30_000 - PLAYBACK_DELAY_MS);
    expect(session.timeline.cues.length).toBeGreaterThan(1);
    expect(session.timeline.cues.at(-1)?.at_ms).toBeLessThanOrEqual(30_000);
  });

  it('subscribing after end delivers the final timeline', () => {
    const { session, pushed, skip } = open();
    skip(END_T + 1);
    session.subscribe((timeline) => pushed.push(timeline));
    expect(pushed).toEqual([FULL]);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('subscribing with nothing new delivers nothing', () => {
    const { session, pushed } = open(1000);
    const stop = session.subscribe((timeline) => pushed.push(timeline));
    expect(pushed).toEqual([]);
    stop();
  });

  it('after unsubscribing nothing arrives and no timer is left', () => {
    const { session, pushed, advance } = open();
    const stop = session.subscribe((timeline) => pushed.push(timeline));
    advance(10_000);
    const count = pushed.length;
    stop();
    expect(vi.getTimerCount()).toBe(0);
    advance(10_000);
    expect(pushed.length).toBe(count);
  });
});

describe('durationStats', () => {
  it('gives nearest-rank percentiles and zeros without samples', () => {
    expect(durationStats([])).toEqual({ count: 0, p50: 0, p95: 0, max: 0 });
    const samples = Array.from({ length: 100 }, (_, i) => 100 - i);
    expect(durationStats(samples)).toEqual({ count: 100, p50: 50, p95: 95, max: 100 });
  });
});
