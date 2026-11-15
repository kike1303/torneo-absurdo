import type { BracketMatch } from '@torneo/protocol';
import { describe, expect, it } from 'vitest';
import { bracketRounds } from './bracket.js';

const match = (
  round: number,
  slot: number,
  a: string,
  b: string,
  advances: 'a' | 'b',
): BracketMatch => ({ round, slot, a, b, fight_id: round * 10 + slot, advances });

// Ana beats Beto, Dani beats Carla, and Dani wins the final.
const FOUR = {
  size: 4,
  matches: [
    match(2, 0, 'Ana', 'Dani', 'b'),
    match(1, 1, 'Carla', 'Dani', 'b'),
    match(1, 0, 'Ana', 'Beto', 'a'),
  ],
};

describe('bracketRounds', () => {
  it('hides every later round and every result until revealed', () => {
    const rounds = bracketRounds(FOUR, false);
    expect(rounds.map((r) => [r.round, r.final])).toEqual([
      [1, false],
      [2, true],
    ]);
    expect(rounds[0]?.matches.map((m) => [m.slot, m.a, m.b, m.fightId, m.advances])).toEqual([
      [0, 'Ana', 'Beto', 10, null],
      [1, 'Carla', 'Dani', 11, null],
    ]);
    expect(rounds[1]?.matches).toEqual([
      { slot: 0, a: null, b: null, fightId: null, advances: null },
    ]);
  });

  it('shows everything once revealed', () => {
    const rounds = bracketRounds(FOUR, true);
    expect(rounds[0]?.matches.map((m) => m.advances)).toEqual(['Ana', 'Dani']);
    expect(rounds[1]?.matches[0]).toEqual({
      slot: 0,
      a: 'Ana',
      b: 'Dani',
      fightId: 20,
      advances: 'Dani',
    });
  });

  it('has three rounds for eight', () => {
    expect(bracketRounds({ size: 8, matches: [] }, false).map((r) => r.final)).toEqual([
      false,
      false,
      true,
    ]);
  });
});
