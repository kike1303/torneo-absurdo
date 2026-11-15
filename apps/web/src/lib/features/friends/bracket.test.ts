import type { BracketMatch, FriendTournament } from '@torneo/protocol';
import { describe, expect, it } from 'vitest';
import { bracketRounds, hiddenView, parseTournament } from './bracket.js';

const match = (
  round: number,
  slot: number,
  a: string,
  b: string,
  advances: 'a' | 'b',
): BracketMatch => ({ round, slot, a, b, fight_id: round * 10 + slot, advances });

// Ana beats Beto, Dani beats Carla, and Dani wins the final.
const FOUR: FriendTournament = {
  id: 7,
  size: 4,
  owner: 'Ana',
  players: ['Ana', 'Beto', 'Carla', 'Dani'],
  champion: 'Dani',
  created_at: '1794700000000',
  matches: [
    match(2, 0, 'Ana', 'Dani', 'b'),
    match(1, 1, 'Carla', 'Dani', 'b'),
    match(1, 0, 'Ana', 'Beto', 'a'),
  ],
};

describe('hiddenView', () => {
  it('keeps round 1 and no outcome at all', () => {
    const view = hiddenView(FOUR);
    expect(JSON.stringify(view)).not.toContain('champion');
    expect(JSON.stringify(view)).not.toContain('advances');
    expect(view.matches.map((m) => [m.round, m.slot])).toEqual([
      [1, 1],
      [1, 0],
    ]);
  });
});

describe('bracketRounds', () => {
  it('shows round 1 and leaves the rest to be decided while hidden', () => {
    const rounds = bracketRounds(hiddenView(FOUR), false);
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

  it('hides later rounds of a whole tournament until revealed', () => {
    expect(bracketRounds(FOUR, false)[1]?.matches[0]?.a).toBeNull();
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

  it('has 4, 2 and 1 places for eight', () => {
    expect(bracketRounds({ size: 8, matches: [] }, false).map((r) => r.matches.length)).toEqual([
      4, 2, 1,
    ]);
  });
});

describe('parseTournament', () => {
  it('takes only the tournament asked for, with its shape', () => {
    expect(parseTournament(FOUR, 7)).toBe(FOUR);
    expect(parseTournament(FOUR, 8)).toBeNull();
    expect(parseTournament({ ...FOUR, champion: 3 }, 7)).toBeNull();
    expect(parseTournament(null, 7)).toBeNull();
  });
});
