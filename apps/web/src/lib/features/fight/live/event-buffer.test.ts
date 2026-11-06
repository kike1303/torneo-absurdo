import { describe, expect, it } from 'vitest';
import type { FightEvent } from '@torneo/protocol';
import { createEventBuffer } from './event-buffer.js';

// 010 DP-B9 and plan-d decision 3.
const event = (t: number): FightEvent => ({ type: 'low_hp', t, side: 'a', hp: 1 });
const page = (from: number, to: number) =>
  Array.from({ length: to - from }, (_, i) => event(from + i));
const times = (events: readonly FightEvent[]) => events.map((e) => e.t);

describe('createEventBuffer', () => {
  it('places consecutive pages of any size', () => {
    const buffer = createEventBuffer();
    expect(buffer.place(0, page(0, 3))).toBe('ok');
    expect(buffer.place(3, page(3, 4))).toBe('ok');
    expect(buffer.place(4, page(4, 11))).toBe('ok');
    expect(times(buffer.events)).toEqual(page(0, 11).map((e) => e.t));
  });

  it('discards the overlap and keeps the rest', () => {
    const buffer = createEventBuffer();
    buffer.place(0, page(0, 5));
    expect(buffer.place(3, page(3, 8))).toBe('ok');
    expect(times(buffer.events)).toEqual([0, 1, 2, 3, 4, 5, 6, 7]);
  });

  it('a page it already has changes nothing, also after a reconnection from 0', () => {
    const buffer = createEventBuffer();
    buffer.place(0, page(0, 5));
    expect(buffer.place(1, page(1, 3))).toBe('ok');
    expect(buffer.place(0, page(0, 5))).toBe('ok');
    expect(times(buffer.events)).toEqual([0, 1, 2, 3, 4]);
  });

  it('a gap adds nothing', () => {
    const buffer = createEventBuffer();
    buffer.place(0, page(0, 2));
    expect(buffer.place(3, page(3, 5))).toBe('gap');
    expect(buffer.events).toHaveLength(2);
  });

  it('past the guard of the engine it adds nothing', () => {
    const buffer = createEventBuffer(4);
    buffer.place(0, page(0, 3));
    expect(buffer.place(3, page(3, 6))).toBe('overflow');
    expect(buffer.events).toHaveLength(3);
    expect(buffer.place(3, page(3, 4))).toBe('ok');
  });
});
