import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import spike from '../fixtures/spike.timeline.json' with { type: 'json' };
import {
  CUE_TYPES,
  CUE_TYPES_V1,
  ENDINGS,
  TIMELINE_VERSION,
  TIMELINE_VERSIONS,
  validateTimeline,
} from '../src/timeline/index.ts';
import { applyOps, loadFixtures } from './support/patch.ts';
import { MANIFESTS } from './support/placeholders.ts';

// 005 RF1 to RF4: timeline v2, a strict superset of v1 with the endings by time.
const { valid, variants, rules } = loadFixtures(
  fileURLToPath(new URL('./fixtures/timeline-v2/', import.meta.url)),
);
const v1 = loadFixtures(fileURLToPath(new URL('./fixtures/timeline/', import.meta.url)));

const asVersion = (json: unknown, version: number): unknown =>
  applyOps(json, [{ op: 'set', path: '/timeline_version', value: version }]);

describe('timeline v2 types (RF1)', () => {
  it('lists the versions, cue types and endings of the spec', () => {
    expect(TIMELINE_VERSIONS).toEqual([1, 2]);
    expect(TIMELINE_VERSION).toBe(2);
    expect(CUE_TYPES_V1).toEqual(['intro', 'strike', 'knockdown', 'ko', 'win']);
    expect(CUE_TYPES).toEqual(['intro', 'strike', 'knockdown', 'ko', 'win', 'time_up', 'draw']);
    expect(ENDINGS).toEqual(['ko', 'decision', 'draw']);
  });
});

describe('validateTimeline v2 (RF2, RF3)', () => {
  it('accepts a decision, with and without manifests, and keeps the version', () => {
    const result = validateTimeline(valid, MANIFESTS);
    expect(result).toMatchObject({ ok: true, timeline: { timeline_version: 2 } });
    expect(validateTimeline(valid)).toMatchObject({ ok: true });
  });

  it('has the draw and KO variants', () => {
    expect(variants.map((v) => v.name).sort()).toEqual(['draw', 'ko']);
  });

  it.each(variants.map((v) => [v.name, v] as const))('accepts the %s variant', (_name, v) => {
    const result = validateTimeline(applyOps(valid, v.ops), MANIFESTS);
    expect(result.ok ? 'ok' : result.errors).toBe('ok');
  });

  it.each(rules.map((r) => [r.name, r] as const))('%s fails with its code', (_name, rule) => {
    const result = validateTimeline(applyOps(valid, rule.ops), MANIFESTS);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual(expect.objectContaining(rule.expect));
  });

  it('gives only ending_conflict for a win and a draw after time_up', () => {
    const rule = rules.find((r) => r.name === 'ending_conflict');
    const result = validateTimeline(applyOps(valid, rule?.ops ?? []));
    expect(result).toEqual({
      ok: false,
      errors: [{ code: 'timeline.ending_conflict', path: '/cues/8' }],
    });
  });

  it('does not report ko_count nor win_count by absence (ending_missing, closing_missing)', () => {
    for (const name of ['ending_missing', 'closing_missing']) {
      const rule = rules.find((r) => r.name === name);
      const result = validateTimeline(applyOps(valid, rule?.ops ?? []));
      expect(result.ok).toBe(false);
      if (result.ok) continue;
      const codes = result.errors.map((e) => e.code);
      expect(codes).not.toContain('timeline.ko_count');
      expect(codes).not.toContain('timeline.win_count');
    }
  });

  it('rejects time_up and draw in a v1 with unknown_cue_type', () => {
    const result = validateTimeline(asVersion(valid, 1));
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual({
      code: 'timeline.unknown_cue_type',
      path: '/cues/6/type',
    });
  });

  it.each([0, 3, '2', null])('rejects timeline_version %s', (version) => {
    expect(validateTimeline(asVersion(valid, version as number))).toEqual({
      ok: false,
      errors: [{ code: 'timeline.unsupported_version', path: '/timeline_version' }],
    });
  });
});

describe('v2 is a superset of v1 (RF4)', () => {
  it.each([
    ['spike script', spike],
    ['v1 _valid.json', v1.valid],
  ])('%s passes as v1 and as v2 with the same timeline', (_name, json) => {
    const one = validateTimeline(asVersion(json, 1), MANIFESTS);
    const two = validateTimeline(asVersion(json, 2), MANIFESTS);
    if (!one.ok || !two.ok) throw new Error(JSON.stringify([one, two]));
    expect(one.timeline.timeline_version).toBe(1);
    expect(two.timeline.timeline_version).toBe(2);
    expect({ ...two.timeline, timeline_version: 1 }).toEqual(one.timeline);
  });
});
