import { describe, expect, it } from 'vitest';
import { stageLooks } from './fighter.js';

describe('stageLooks', () => {
  it('gives the stage what each side wore, and nothing of a side without a look', () => {
    expect(
      stageLooks({
        look_a: { idle: 'idle_3', victory: null, accessories: ['oven_mitts'], skin: null },
        look_b: null,
      }),
    ).toEqual({ cosmetics: { a: { idle: 'idle_3' } }, looks: { a: { wear: ['oven_mitts'] } } });
    expect(
      stageLooks({
        look_a: { idle: null, victory: null, accessories: [], skin: null },
        look_b: { idle: null, victory: 'win_1', accessories: [], skin: null },
      }),
    ).toEqual({ cosmetics: { a: {}, b: { victory: 'win_1' } }, looks: {} });
  });

  it('dresses a side in its suit, with or without accessories (024)', () => {
    expect(
      stageLooks({
        look_a: { idle: null, victory: null, accessories: [], skin: 'tournament_suit' },
        look_b: { idle: null, victory: null, accessories: ['oven_mitts'], skin: 'tournament_suit' },
      }).looks,
    ).toEqual({
      a: { wear: [], skin: 'tournament_suit' },
      b: { wear: ['oven_mitts'], skin: 'tournament_suit' },
    });
  });
});
