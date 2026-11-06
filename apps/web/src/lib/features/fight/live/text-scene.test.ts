import { describe, expect, it } from 'vitest';
import type { FightEvent } from '@torneo/protocol';
import { endEvent, hasPendingEvents, textSceneAt } from './text-scene.js';

// 010 RF54 and plan-d decision 9.
const MAX = { a: 100, b: 100 };
const EVENTS: FightEvent[] = [
  { type: 'intro', t: 0 },
  { type: 'attack', t: 2000, attacker: 'a', ability: 'x', kind: 'strike' },
  {
    type: 'hit',
    t: 2400,
    attacker: 'a',
    ability: 'x',
    damage: 10,
    crit: false,
    weak: false,
    hp: 90,
  },
  { type: 'block', t: 3000, attacker: 'b', ability: 'y', damage: 4, hp: 96 },
  { type: 'dodge', t: 4000, attacker: 'a', ability: 'x' },
  { type: 'ko', t: 5000, side: 'b' },
  { type: 'end', t: 9000, winner: 'a', reason: 'ko' },
];

describe('textSceneAt', () => {
  it('shows the HP of that moment, not of what arrived', () => {
    expect(textSceneAt(MAX, EVENTS, 0).hp).toEqual(MAX);
    expect(textSceneAt(MAX, EVENTS, 2399).hp).toEqual(MAX);
    expect(textSceneAt(MAX, EVENTS, 2400).hp).toEqual({ a: 100, b: 90 });
    expect(textSceneAt(MAX, EVENTS, 3000).hp).toEqual({ a: 96, b: 90 });
  });

  it('a result only from t ≥ end.t, never for having received end', () => {
    expect(textSceneAt(MAX, EVENTS, 8999).ended).toBeNull();
    expect(textSceneAt(MAX, EVENTS, 9000).ended).toEqual({ winner: 'a', reason: 'ko' });
  });
});

describe('endEvent', () => {
  it('is the last event, if it is the end', () => {
    expect(endEvent(EVENTS)?.t).toBe(9000);
    expect(endEvent(EVENTS.slice(0, -1))).toBeNull();
    expect(endEvent([])).toBeNull();
  });
});

describe('hasPendingEvents', () => {
  it('says whether a received event is still ahead', () => {
    expect(hasPendingEvents(EVENTS, 8999)).toBe(true);
    expect(hasPendingEvents(EVENTS, 9000)).toBe(false);
    expect(hasPendingEvents([], 0)).toBe(false);
  });
});
