import { describe, expect, it } from 'vitest';
import { validateManifest } from '@torneo/renderer/manifest';
import { validateTimeline, type Timeline } from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import spikeJson from '@torneo/renderer/fixtures/spike.timeline.json';
import { meanStrikeGapMs, parsePace, retimeTimeline } from '../src/lib/features/arena/retime.js';
import { timelineFromFightLog } from '../src/lib/features/fight/timeline-from-fight-log.js';
import { TIME_LOGS, readGoldenLog } from './support/golden-logs.js';

// 002 RF-B9, plan-b decision 17, and 005 RF17. Here and not next to retime.ts: it reads renderer
// values.
const spike = spikeJson as unknown as Timeline;
const manifests = new Map(
  [dummyA, dummyB].map((json) => {
    const result = validateManifest(json);
    if (!result.ok) throw new Error('placeholder');
    return [result.manifest.id, result.manifest] as const;
  }),
);

describe('parsePace', () => {
  it.each([
    ['1', 1],
    ['0.8', 0.8],
    ['1.25', 1.25],
    ['1.5', 1.5],
  ])('%s is %d', (raw, value) => expect(parsePace(raw)).toBe(value));

  it.each([null, '', '0.79', '1.51', '2', '1.255', '-1', 'abc', '1e0', ' 1'])(
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

  // 005 RF17 (amended): the minimum gap between strikes of the goldens is 700 ms, so 0.8 breaks
  // strike_gap (560 < 600) whatever the ending; 0.86 is the smallest factor that keeps it.
  describe.each(TIME_LOGS)('%s (ends by time)', (name) => {
    const adapted = timelineFromFightLog(readGoldenLog(name), {
      a: { asset: 'dummy_a' },
      b: { asset: 'dummy_b' },
    });
    if (!adapted.ok) throw new Error(JSON.stringify(adapted.errors));
    const timeline = adapted.timeline;
    const find = (t: Timeline, type: string) => t.cues.find((c) => c.type === type);

    it.each([1.5, 0.86])('factor %d passes validateTimeline', (factor) => {
      const result = validateTimeline(retimeTimeline(timeline, factor), manifests);
      expect(result.ok ? [] : result.errors).toEqual([]);
    });

    it('factor 0.8 only breaks strike_gap', () => {
      const result = validateTimeline(retimeTimeline(timeline, 0.8), manifests);
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(new Set(result.errors.map((e) => e.code))).toEqual(new Set(['timeline.strike_gap']));
    });

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
