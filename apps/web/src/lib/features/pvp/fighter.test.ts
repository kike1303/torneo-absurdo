import { describe, expect, it } from 'vitest';
import { stageLooks } from './fighter.js';

describe('stageLooks', () => {
  it('gives the stage what each side wore, and nothing of a side without a look', () => {
    expect(
      stageLooks({
        look_a: { idle: 'idle_3', victory: null, accessories: ['oven_mitts'] },
        look_b: null,
      }),
    ).toEqual({ cosmetics: { a: { idle: 'idle_3' } }, looks: { a: { wear: ['oven_mitts'] } } });
    expect(
      stageLooks({
        look_a: { idle: null, victory: null, accessories: [] },
        look_b: { idle: null, victory: 'win_1', accessories: [] },
      }),
    ).toEqual({ cosmetics: { a: {}, b: { victory: 'win_1' } }, looks: {} });
  });
});
