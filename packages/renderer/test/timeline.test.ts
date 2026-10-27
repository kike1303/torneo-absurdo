import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { TIMELINE_CODES, validateTimeline } from '../src/timeline/index.ts';
import { applyOps, loadFixtures } from './support/patch.ts';
import { MANIFESTS } from './support/placeholders.ts';

// RF-A7: the completed example of the spec passes, and one fixture per rule fails with its code.
const { valid, rules } = loadFixtures(
  fileURLToPath(new URL('./fixtures/timeline/', import.meta.url)),
);

describe('validateTimeline', () => {
  it('accepts the completed example of the spec, with and without manifests', () => {
    expect(validateTimeline(valid)).toMatchObject({ ok: true });
    expect(validateTimeline(valid, MANIFESTS)).toMatchObject({ ok: true });
  });

  it('covers every rule code with a fixture', () => {
    const covered = new Set(rules.map((r) => r.expect.code));
    const structural = new Set<string>([
      TIMELINE_CODES.notObject,
      TIMELINE_CODES.invalidType,
      TIMELINE_CODES.invalidValue,
      TIMELINE_CODES.missingField,
      TIMELINE_CODES.tooManyErrors,
      TIMELINE_CODES.tooManyCues,
    ]);
    const missing = Object.values(TIMELINE_CODES).filter(
      (c) => !covered.has(c) && !structural.has(c),
    );
    expect(missing).toEqual([]);
  });

  it.each(rules.map((r) => [r.name, r] as const))('%s fails with its code', (_name, rule) => {
    const json = applyOps(valid, rule.ops);
    const result = validateTimeline(json, rule.manifests ? MANIFESTS : undefined);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual(expect.objectContaining(rule.expect));
  });

  it('unknown_asset only applies when manifests are given', () => {
    const json = applyOps(valid, [{ op: 'set', path: '/fighters/a/asset', value: 'ghost' }]);
    expect(validateTimeline(json)).toMatchObject({ ok: true });
  });

  it('rejects more than 2000 cues', () => {
    const cues = Array.from({ length: 2001 }, () => ({ at_ms: 0, type: 'intro' }));
    const json = applyOps(valid, [{ op: 'set', path: '/cues', value: cues }]);
    expect(validateTimeline(json)).toEqual({
      ok: false,
      errors: [{ code: 'timeline.too_many_cues', path: '/cues', data: { count: 2001 } }],
    });
  });

  it('rejects bad enum values and types with codes', () => {
    const json = applyOps(valid, [
      { op: 'set', path: '/cues/1/actor', value: 'c' },
      { op: 'set', path: '/cues/2/at_ms', value: 4100.5 },
    ]);
    const result = validateTimeline(json);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual({ code: 'timeline.invalid_value', path: '/cues/1/actor' });
    expect(result.errors).toContainEqual({ code: 'timeline.invalid_type', path: '/cues/2/at_ms' });
  });

  it('stops at 100 errors with too_many_errors', () => {
    const cues = Array.from({ length: 150 }, () => ({ at_ms: 0, type: 'taunt' }));
    const result = validateTimeline(applyOps(valid, [{ op: 'set', path: '/cues', value: cues }]));
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toHaveLength(101);
    expect(result.errors[100]?.code).toBe('timeline.too_many_errors');
  });
});

describe('QA findings', () => {
  const base = {
    timeline_version: 1,
    duration_ms: 8000,
    fighters: {
      a: { asset: 'dummy_a', max_hp: 1000 },
      b: { asset: 'dummy_b', max_hp: 1000 },
    },
  };

  it('rejects stacked knockdowns after a single strike', () => {
    const kds = Array.from({ length: 51 }, (_, i) => ({
      at_ms: 3000 + i * 2,
      type: 'knockdown',
      fighter: 'b',
      duration_ms: 800,
    }));
    const json = {
      ...base,
      cues: [
        { at_ms: 0, type: 'intro' },
        {
          at_ms: 3000,
          type: 'strike',
          actor: 'a',
          move: 'attack_heavy',
          result: 'crit',
          damage: 1000,
          target_hp: 0,
        },
        ...kds,
        { at_ms: 4000, type: 'ko', fighter: 'b' },
        { at_ms: 5500, type: 'win', fighter: 'a' },
      ],
    };
    const result = validateTimeline(json);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual(
      expect.objectContaining({ code: 'timeline.knockdown_duplicate', path: '/cues/3' }),
    );
  });

  it('rejects a ko without a strike that left that fighter at 0 HP', () => {
    const json = {
      ...base,
      cues: [
        { at_ms: 0, type: 'intro' },
        { at_ms: 3000, type: 'ko', fighter: 'b' },
        { at_ms: 4500, type: 'win', fighter: 'a' },
      ],
    };
    const result = validateTimeline(json);
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toContainEqual({ code: 'timeline.ko_without_zero_hp', path: '/cues/1' });
  });
});
