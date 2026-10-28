import { describe, expect, it } from 'vitest';
import { validateManifest } from '@torneo/renderer/manifest';
import { validateTimeline, type Timeline } from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import spikeJson from '@torneo/renderer/fixtures/spike.timeline.json';
import {
  PACE_FLOOR,
  PACE_MAX,
  meanStrikeGapMs,
  paceMin,
  parsePace,
  resolvePace,
  retimeTimeline,
} from '../src/lib/features/arena/retime.js';
import { timelineFromFightLog } from '../src/lib/features/fight/timeline-from-fight-log.js';
import { KO_LOGS, TIME_LOGS, readGoldenLog, type GoldenLogName } from './support/golden-logs.js';

// 002 RF-B9, plan-b decision 17, 005 RF17 and 006 RF14 to RF16. Here and not next to retime.ts: it
// reads renderer values. The validator is always the real one, never a double (006 plan, architect
// decision 1).
const spike = spikeJson as unknown as Timeline;
const isValid = (t: Timeline) => validateTimeline(t).ok;
const manifests = new Map(
  [dummyA, dummyB].map((json) => {
    const result = validateManifest(json);
    if (!result.ok) throw new Error('placeholder');
    return [result.manifest.id, result.manifest] as const;
  }),
);

describe('parsePace (syntax only, 006 RF14)', () => {
  it.each([
    ['0', 0],
    ['0.5', 0.5],
    ['0.79', 0.79],
    ['1', 1],
    ['1.25', 1.25],
    ['1.51', 1.51],
    ['2', 2],
    ['9.99', 9.99],
  ])('%s is %d', (raw, value) => expect(parsePace(raw)).toBe(value));

  it.each([null, '', '10', '1.255', '-1', '.5', '1.', 'abc', '1e0', ' 1', 'NaN', 'Infinity'])(
    '%j is ignored',
    (raw) => {
      expect(parsePace(raw)).toBeNull();
    },
  );
});

describe('retimeTimeline', () => {
  it('factor 1 gives the same timeline', () => {
    expect(retimeTimeline(spike, 1)).toBe(spike);
  });

  it.each([0.8, 1.25, 1.5])('factor %d passes validateTimeline', (factor) => {
    const result = validateTimeline(retimeTimeline(spike, factor), manifests);
    expect(result.ok ? [] : result.errors).toEqual([]);
  });

  it('the mean gap between strikes scales with the factor', () => {
    const base = meanStrikeGapMs(spike) ?? 0;
    expect(base).toBeGreaterThan(0);
    expect(meanStrikeGapMs(retimeTimeline(spike, 1.25))).toBeCloseTo(base * 1.25, -1);
    expect(meanStrikeGapMs(retimeTimeline(spike, 0.8))).toBeCloseTo(base * 0.8, -1);
  });

  it('keeps the knockdown, the ko → win gap and the tail; cues stay integers', () => {
    const out = retimeTimeline(spike, 1.25);
    const find = (t: Timeline, type: string) => t.cues.find((c) => c.type === type)!;
    const kd = find(out, 'knockdown');
    expect(kd.type === 'knockdown' && kd.duration_ms).toBe(
      (find(spike, 'knockdown') as { duration_ms: number }).duration_ms,
    );
    expect(find(out, 'win').at_ms - find(out, 'ko').at_ms).toBe(
      find(spike, 'win').at_ms - find(spike, 'ko').at_ms,
    );
    expect(out.duration_ms - find(out, 'win').at_ms).toBe(
      spike.duration_ms - find(spike, 'win').at_ms,
    );
    expect(out.cues.every((c) => Number.isInteger(c.at_ms))).toBe(true);
    expect(out.cues[0]).toEqual(spike.cues[0]);
  });

  it('meanStrikeGapMs is null with fewer than two strikes', () => {
    expect(meanStrikeGapMs({ cues: [] })).toBeNull();
  });

  describe.each(TIME_LOGS)('%s (ends by time)', (name) => {
    const timeline = adapt(name);
    const find = (t: Timeline, type: string) => t.cues.find((c) => c.type === type);

    it.each([0.8, 0.86, 1.5])(
      'factor %d scales time_up and keeps the closing gap and tail',
      (f) => {
        const out = retimeTimeline(timeline, f);
        const first = find(timeline, 'strike')?.at_ms ?? 0;
        const timeUp = find(timeline, 'time_up')?.at_ms ?? 0;
        const closing = timeline.cues.at(-1);
        const outClosing = out.cues.at(-1);
        expect(outClosing?.type).toBe(closing?.type);
        expect(find(out, 'time_up')?.at_ms).toBe(Math.round(first + (timeUp - first) * f));
        expect((outClosing?.at_ms ?? 0) - (find(out, 'time_up')?.at_ms ?? 0)).toBe(
          (closing?.at_ms ?? 0) - timeUp,
        );
        expect(out.duration_ms - (outClosing?.at_ms ?? 0)).toBe(
          timeline.duration_ms - (closing?.at_ms ?? 0),
        );
      },
    );
  });
});

function adapt(name: GoldenLogName): Timeline {
  const adapted = timelineFromFightLog(readGoldenLog(name), {
    a: { asset: 'dummy_a' },
    b: { asset: 'dummy_b' },
  });
  if (!adapted.ok) throw new Error(JSON.stringify(adapted.errors));
  return adapted.timeline;
}

/** Every hundredth of [from, to], built as k / 100 like paceMin. */
function hundredths(from: number, to: number): number[] {
  const out: number[] = [];
  for (let k = Math.round(from * 100); k <= Math.round(to * 100); k++) out.push(k / 100);
  return out;
}

const SOURCES: readonly (readonly [string, Timeline])[] = [
  ...[...KO_LOGS, ...TIME_LOGS].map((name) => [name, adapt(name)] as const),
  ['spike', spike],
];

describe('paceMin (006 RF15)', () => {
  // The minimum gap between strikes of the goldens is 700 ms: 0.86 turns it into 602 >= 600.
  it.each([...KO_LOGS, ...TIME_LOGS])('%s has 0.86 as its minimum', (name) => {
    expect(paceMin(adapt(name), isValid)).toBe(0.86);
  });

  describe.each(SOURCES)('%s', (_, timeline) => {
    const min = paceMin(timeline, isValid);

    it('has a minimum in [PACE_FLOOR, 1]', () => {
      expect(min).not.toBeNull();
      expect(min).toBeGreaterThanOrEqual(PACE_FLOOR);
      expect(min).toBeLessThanOrEqual(1);
    });

    it('every hundredth of [min, PACE_MAX] passes with the placeholder manifests', () => {
      const broken = hundredths(min ?? 1, PACE_MAX).filter(
        (f) => !validateTimeline(retimeTimeline(timeline, f), manifests).ok,
      );
      expect(broken).toEqual([]);
    });

    it('min - 0.01 only breaks strike_gap or strike_during_knockdown', () => {
      if (min === null || min <= PACE_FLOOR) return;
      const below = (Math.round(min * 100) - 1) / 100;
      const result = validateTimeline(retimeTimeline(timeline, below), manifests);
      expect(result.ok).toBe(false);
      if (result.ok) return;
      const allowed = new Set(['timeline.strike_gap', 'timeline.strike_during_knockdown']);
      expect(result.errors.map((e) => e.code).filter((c) => !allowed.has(c))).toEqual([]);
    });
  });

  it('a timeline invalid with factor 1 gives null without throwing', () => {
    const invalid = tooCloseStrikes(spike);
    expect(isValid(invalid)).toBe(false);
    expect(paceMin(invalid, isValid)).toBeNull();
  });
});

describe('resolvePace (006 RF16)', () => {
  const timeline = adapt('log-ko-knockdown');

  it.each([
    [0, 0.86, true],
    [0.5, 0.86, true],
    [0.8, 0.86, true],
    [0.85, 0.86, true],
    [0.86, 0.86, false],
    [1, 1, false],
    [1.25, 1.25, false],
    [1.5, 1.5, false],
    [1.51, 1.5, true],
    [9.99, 1.5, true],
  ])('%d resolves to %d (clamped: %s)', (requested, effective, clamped) => {
    expect(resolvePace(requested, timeline, isValid)).toEqual({
      requested,
      min: 0.86,
      effective,
      clamped,
    });
  });

  it.each([
    [1, false],
    [0.8, true],
    [1.5, true],
  ])('with an invalid timeline, %d gives effective 1 (clamped: %s)', (requested, clamped) => {
    expect(resolvePace(requested, tooCloseStrikes(spike), isValid)).toEqual({
      requested,
      min: null,
      effective: 1,
      clamped,
    });
  });
});

/** The timeline with its second strike moved 1 ms after the first: invalid at every factor. */
function tooCloseStrikes(timeline: Timeline): Timeline {
  const strikes = timeline.cues.filter((c) => c.type === 'strike');
  const [first, second] = strikes;
  if (first === undefined || second === undefined) throw new Error('two strikes');
  return {
    ...timeline,
    cues: timeline.cues.map((c) => (c === second ? { ...c, at_ms: first.at_ms + 1 } : c)),
  };
}
