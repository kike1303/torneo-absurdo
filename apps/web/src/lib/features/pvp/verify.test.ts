import { describe, expect, it, vi } from 'vitest';
import type { VerifyDeps, VerifyEngine } from '../replay/verify.js';
import { parsePvpDetail, verifyPvpFight } from './verify.js';

const SEED = 'ab'.repeat(32);
const SPEC_A = '{"slug":"player-1"}';
const SPEC_B = '{"slug":"rata-electrica"}';
const LOG = JSON.stringify({
  ruleset_version: 2,
  a: { slug: 'player-1', max_hp: 100 },
  b: { slug: 'rata-electrica', max_hp: 100 },
  events: [{ type: 'end', t: 9, winner: 'a', reason: 'ko' }],
});

function detail(overrides: Record<string, unknown> = {}) {
  return {
    id: 7,
    a: { kind: 'player', display_name: 'Ana' },
    b: { kind: 'tournament', slug: 'rata-electrica' },
    spec_a: SPEC_A,
    spec_b: SPEC_B,
    seed: SEED,
    log: LOG,
    ruleset_version: 2,
    winner: 'a',
    end_reason: 'ko',
    rating_delta: 0,
    look_a: { idle: null, victory: 'win_2', accessories: ['oven_mitts'] },
    look_b: null,
    created_at: '1700000000000',
    ...overrides,
  };
}

function deps(simulate: VerifyEngine['simulate']): VerifyDeps {
  return {
    sha256: () => Promise.reject(new Error('unused')),
    engine: () => Promise.resolve({ simulate, rulesetBounds: () => '{}' }),
  };
}

const honest = deps(() => LOG);

describe('verifyPvpFight', () => {
  it('matches a fight the engine reproduces, from its own seed and specs', async () => {
    const simulate = vi.fn(() => LOG);
    expect(await verifyPvpFight(detail(), 7, deps(simulate))).toEqual({
      status: 'match',
      scope: 'fight',
    });
    expect(simulate).toHaveBeenCalledWith(SEED, SPEC_A, SPEC_B, 2);
  });

  it('says what does not hold', async () => {
    const cases: [unknown, number, VerifyDeps, string][] = [
      [detail(), 8, honest, 'data'],
      [{ ...detail(), seed: null }, 7, honest, 'data'],
      [detail({ seed: 'XY'.repeat(32) }), 7, honest, 'commit'],
      [detail(), 7, deps(() => `${LOG} `), 'log'],
      [detail({ winner: 'b' }), 7, honest, 'final'],
      [detail({ end_reason: 'decision' }), 7, honest, 'final'],
      [detail({ b: { kind: 'tournament', slug: 'otro' } }), 7, honest, 'summary'],
      [detail({ spec_a: '{"slug":"player-9"}' }), 7, honest, 'summary'],
    ];
    for (const [body, id, d, check] of cases) {
      expect(await verifyPvpFight(body, id, d), check).toEqual({ status: 'mismatch', check });
    }
  });

  it('says when the browser could not check, and never throws', async () => {
    const unavailable: VerifyDeps = {
      sha256: () => Promise.reject(new Error('unused')),
      engine: () => Promise.reject(new Error('timeout')),
    };
    expect(await verifyPvpFight(detail(), 7, unavailable)).toEqual({
      status: 'cannot_check',
      reason: 'engine_unavailable',
    });
    const unknown = deps(() => {
      throw new Error('ruleset.unknown');
    });
    expect(await verifyPvpFight(detail(), 7, unknown)).toEqual({
      status: 'cannot_check',
      reason: 'ruleset_unknown',
    });
    const badData = deps(() => {
      throw new Error('spec.invalid');
    });
    expect(await verifyPvpFight(detail(), 7, badData)).toEqual({
      status: 'mismatch',
      check: 'log',
    });
    const trap = deps(() => {
      throw new Error('unreachable');
    });
    expect(await verifyPvpFight(detail(), 7, trap)).toEqual({
      status: 'cannot_check',
      reason: 'engine_failed',
    });
  });

  it('parses only the shape of the protocol', () => {
    expect(parsePvpDetail(detail())).not.toBeNull();
    expect(parsePvpDetail(detail({ winner: null, end_reason: 'draw' }))).not.toBeNull();
    // A suit (024), and none in a look of before.
    expect(
      parsePvpDetail(
        detail({ look_a: { idle: null, victory: null, accessories: [], skin: 'tournament_suit' } }),
      ),
    ).not.toBeNull();
    expect(
      parsePvpDetail(detail({ look_a: { idle: null, victory: null, accessories: [] } })),
    ).not.toBeNull();
    for (const bad of [
      null,
      [],
      detail({ a: { kind: 'player' } }),
      detail({ b: { kind: 'robot', slug: 'x' } }),
      detail({ winner: 'c' }),
      detail({ end_reason: 'forfeit' }),
      detail({ log: null }),
      detail({ id: '7' }),
      detail({ look_a: { idle: 3, victory: null, accessories: [] } }),
      detail({ look_b: { idle: null, victory: null, accessories: [1] } }),
      detail({ look_a: undefined }),
      detail({ look_a: { idle: 'dance', victory: null, accessories: [] } }),
      detail({ look_a: { idle: null, victory: null, accessories: [], skin: 7 } }),
    ]) {
      expect(parsePvpDetail(bad)).toBeNull();
    }
  });
});
