import { describe, expect, it } from 'vitest';
import { guideTable, windupCap, type RulesetTiming } from '../src/guide/index.ts';
import { OPTIONAL_ANIMATIONS, REQUIRED_ANIMATIONS } from '../src/manifest/index.ts';
import { V1, v2 } from './support/guide-ref.ts';

// 007 RF-A4 (windupCap) and RF-A6 (guideTable).
const MOVES = ['attack_light', 'attack_heavy', 'special'] as const;
const caps = (rulesets: readonly RulesetTiming[]) => MOVES.map((m) => windupCap(m, rulesets));
const row = (rulesets: readonly RulesetTiming[], animation: string) =>
  guideTable(rulesets).find((r) => r.animation === animation);

describe('windupCap', () => {
  it('is 400, 400 and 400 with v1', () => {
    expect(caps([V1])).toEqual([400, 400, 400]);
  });

  it('takes the minimum over the versions, in any order', () => {
    const V2 = v2({ windup_strike_ms: 300, windup_heavy_ms: 700, windup_special_ms: 350 });
    expect(caps([V1, V2])).toEqual([300, 400, 350]);
    expect(caps([V2, V1])).toEqual([300, 400, 350]);
  });

  it('throws without rulesets', () => {
    expect(() => windupCap('attack_light', [])).toThrow(new RangeError('guide.no_rulesets'));
  });
});

describe('the windup range of the table', () => {
  it('has no version when the structural cap limits (v1)', () => {
    expect(row([V1], 'attack_light')?.windup_ms).toEqual({ min: 100, max: 250 });
    expect(row([V1], 'attack_heavy')?.windup_ms).toEqual({ min: 250, max: 400 });
    expect(row([V1], 'special')?.windup_ms).toEqual({ min: 300, max: 400 });
  });

  it('carries the version when the ruleset is strictly lower', () => {
    const V2 = v2({ windup_heavy_ms: 280 });
    expect(row([V1, V2], 'attack_heavy')?.windup_ms).toEqual({
      min: 250,
      max: 280,
      ruleset_version: 2,
    });
  });

  it('reports the lowest version on a tie between versions', () => {
    const V2 = v2({ windup_heavy_ms: 280 });
    const V3 = { ...V2, ruleset_version: 3 };
    expect(row([V3, V1, V2], 'attack_heavy')?.windup_ms).toEqual({
      min: 250,
      max: 280,
      ruleset_version: 2,
    });
  });

  it('has no version on a tie with the structural cap', () => {
    expect(row([V1, v2({ windup_heavy_ms: 400 })], 'attack_heavy')?.windup_ms).toEqual({
      min: 250,
      max: 400,
    });
    expect(row([V1, v2({ windup_strike_ms: 250 })], 'attack_light')?.windup_ms).toEqual({
      min: 100,
      max: 250,
    });
  });
});

describe('guideTable', () => {
  it('gives the approved table with v1', () => {
    const attack = { recovery_min_ms: 200 };
    const reaction = { duration_ms: { min: 200, max: 1000 }, first_frame_max_ms: 134 };
    expect(guideTable([V1])).toEqual([
      {
        animation: 'idle',
        required: true,
        frames: { min: 4, max: 12 },
        distinct_min: 3,
        frame_max_ms: 250,
        cycle_ms: { min: 600, max: 2400 },
      },
      {
        animation: 'attack_light',
        required: true,
        frames: { min: 3, max: 8 },
        windup_ms: { min: 100, max: 250 },
        impact_frame_min: 1,
        impact_hold_min_ms: 50,
        ...attack,
      },
      {
        animation: 'attack_heavy',
        required: true,
        frames: { min: 4, max: 10 },
        windup_ms: { min: 250, max: 400 },
        impact_frame_min: 2,
        impact_hold_min_ms: 66,
        ...attack,
      },
      {
        animation: 'special',
        required: true,
        frames: { min: 5, max: 14 },
        windup_ms: { min: 300, max: 400 },
        impact_frame_min: 3,
        impact_hold_min_ms: 66,
        ...attack,
      },
      { animation: 'hit', required: true, frames: { min: 2, max: 6 }, ...reaction },
      { animation: 'block', required: true, frames: { min: 2, max: 6 }, ...reaction },
      { animation: 'dodge', required: true, frames: { min: 2, max: 6 }, ...reaction },
      {
        animation: 'ko',
        required: true,
        frames: { min: 3, max: 12 },
        duration_ms: { min: 0, max: 1500 },
      },
      { animation: 'win', required: true, frames: { min: 2, max: 12 } },
      {
        animation: 'intro',
        required: false,
        frames: { min: 3, max: 24 },
        duration_ms: { min: 0, max: 1600 },
      },
      {
        animation: 'walk',
        required: false,
        frames: { min: 4, max: 12 },
        cycle_ms: { min: 300, max: 1200 },
      },
      {
        animation: 'knockdown',
        required: false,
        frames: { min: 3, max: 12 },
        duration_ms: { min: 0, max: 1500 },
      },
    ]);
  });

  it('is in the canonical order', () => {
    expect(guideTable([V1]).map((r) => r.animation)).toEqual([
      ...REQUIRED_ANIMATIONS,
      ...OPTIONAL_ANIMATIONS,
    ]);
  });

  it('caps knockdown with the lowest knockdown_ms', () => {
    expect(row([V1, v2({ knockdown_ms: 1000 })], 'knockdown')?.duration_ms).toEqual({
      min: 0,
      max: 1000,
    });
  });

  it('holds only numbers, booleans and animation codes, never text', () => {
    const codes: readonly string[] = [...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS];
    const leaves: unknown[] = [];
    const walk = (value: unknown, key: string): void => {
      if (typeof value === 'object' && value !== null) {
        for (const [k, v] of Object.entries(value)) walk(v, k);
      } else {
        leaves.push(value);
        if (typeof value === 'string') {
          expect(key).toBe('animation');
          expect(codes).toContain(value);
        } else {
          expect(['number', 'boolean']).toContain(typeof value);
        }
      }
    };
    walk(guideTable([V1, v2({ windup_heavy_ms: 280 })]), '');
    expect(leaves.length).toBeGreaterThan(0);
  });

  it('throws without rulesets', () => {
    expect(() => guideTable([])).toThrow(new RangeError('guide.no_rulesets'));
  });
});
