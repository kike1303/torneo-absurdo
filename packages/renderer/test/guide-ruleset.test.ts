import { describe, expect, it } from 'vitest';
import { GUIDE_LIMITS, parseRulesetTiming, windupCap } from '../src/guide/index.ts';
import { REACTION_WINDOW_MS } from '../src/manifest/index.ts';
import { GOLDEN, goldenJson } from './support/guide-ref.ts';

// 007 RF-A5 (TS side) and plan-a decision 6: the golden parses and the guide can be met with
// every version it lists.
const TIMING = {
  ruleset_version: 1,
  strike_gap_min_ms: 700,
  windup_strike_ms: 400,
  windup_heavy_ms: 700,
  windup_special_ms: 1100,
  knockdown_ms: 1500,
};

const invalid = (path: string) => ({ ok: false, errors: [{ code: 'ruleset.invalid', path }] });

describe('parseRulesetTiming', () => {
  it('accepts the real golden', () => {
    const result = parseRulesetTiming(goldenJson());
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.rulesets.length).toBeGreaterThan(0);
    expect(result.rulesets.find((r) => r.ruleset_version === 1)).toEqual(TIMING);
  });

  it('accepts several sorted versions', () => {
    const two = { ...TIMING, ruleset_version: 2 };
    expect(parseRulesetTiming({ versions: [TIMING, two] })).toEqual({
      ok: true,
      rulesets: [TIMING, two],
    });
  });

  it.each<[string, unknown, string]>([
    ['an extra key', { versions: [{ ...TIMING, extra: 1 }] }, '/versions/0/extra'],
    [
      'a missing key',
      { versions: [{ ...TIMING, knockdown_ms: undefined }] },
      '/versions/0/knockdown_ms',
    ],
    [
      'a missing strike_gap_min_ms (a golden older than 007 phase B)',
      { versions: [{ ...TIMING, strike_gap_min_ms: undefined }] },
      '/versions/0/strike_gap_min_ms',
    ],
    ['a repeated version', { versions: [TIMING, TIMING] }, '/versions/1/ruleset_version'],
    [
      'unsorted versions',
      { versions: [{ ...TIMING, ruleset_version: 2 }, TIMING] },
      '/versions/1/ruleset_version',
    ],
    ['no versions', { versions: [] }, '/versions'],
    ['versions that are not an array', { versions: {} }, '/versions'],
    ['a missing versions key', {}, '/versions'],
    [
      'a negative value',
      { versions: [{ ...TIMING, windup_strike_ms: -1 }] },
      '/versions/0/windup_strike_ms',
    ],
    [
      'a non-integer value',
      { versions: [{ ...TIMING, windup_heavy_ms: 1.5 }] },
      '/versions/0/windup_heavy_ms',
    ],
    [
      'a string value',
      { versions: [{ ...TIMING, knockdown_ms: '1500' }] },
      '/versions/0/knockdown_ms',
    ],
    ['an entry that is not an object', { versions: [1] }, '/versions/0'],
  ])('rejects %s', (_name, json, path) => {
    // JSON.parse(JSON.stringify(...)) drops the `undefined` keys, as a real file would not have them.
    expect(parseRulesetTiming(JSON.parse(JSON.stringify(json)))).toEqual(invalid(path));
  });

  it('rejects an extra key at the root', () => {
    expect(parseRulesetTiming({ versions: [TIMING], extra: true })).toEqual(invalid('/extra'));
  });

  it.each([null, [], 'x', 1])('rejects a root that is not an object (%j)', (json) => {
    expect(parseRulesetTiming(json)).toEqual(invalid(''));
  });

  it('caps the number of errors', () => {
    const versions = Array.from({ length: 150 }, () => 1);
    const result = parseRulesetTiming({ versions });
    expect(result.ok).toBe(false);
    if (result.ok) return;
    expect(result.errors).toHaveLength(101);
    expect(result.errors.at(-1)).toEqual({ code: 'ruleset.invalid', path: '' });
  });
});

describe('the guide can be met with every version of the golden', () => {
  const A = GUIDE_LIMITS.animations;
  it.each(GOLDEN.map((r) => [r.ruleset_version, r] as const))('ruleset v%i', (_v, r) => {
    const v = { version: r.ruleset_version };
    const cap = (m: 'attack_light' | 'attack_heavy' | 'special') => windupCap(m, [r]);
    expect({ ...v, light: cap('attack_light') >= A.attack_light.windupMinMs }).toEqual({
      ...v,
      light: true,
    });
    expect({ ...v, heavy: cap('attack_heavy') >= A.attack_heavy.windupMinMs }).toEqual({
      ...v,
      heavy: true,
    });
    expect({ ...v, special: cap('special') >= A.special.windupMinMs }).toEqual({
      ...v,
      special: true,
    });
    expect({
      ...v,
      order: cap('attack_heavy') >= A.attack_light.windupMinMs + GUIDE_LIMITS.windupStepMs,
    }).toEqual({ ...v, order: true });
    expect({
      ...v,
      knockdown: r.knockdown_ms >= A.knockdown.framesMin * GUIDE_LIMITS.frameMsMin,
    }).toEqual({ ...v, knockdown: true });
    // The recommended recovery (007 plan-b decision 5) never falls below the validated minimum.
    expect({ ...v, recovery: r.strike_gap_min_ms >= REACTION_WINDOW_MS }).toEqual({
      ...v,
      recovery: true,
    });
  });
});
