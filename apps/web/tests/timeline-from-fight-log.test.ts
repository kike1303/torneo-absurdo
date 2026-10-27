import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import type { FightEvent, FightLog } from '@torneo/protocol';
import { validateManifest } from '@torneo/renderer/manifest';
import {
  TIMELINE_LIMITS,
  TIMELINE_VERSION,
  validateTimeline,
  type Timeline,
} from '@torneo/renderer/timeline';
import dummyA from '@torneo/renderer/placeholders/dummy_a/manifest.json';
import dummyB from '@torneo/renderer/placeholders/dummy_b/manifest.json';
import {
  WIN_DELAY_MS,
  timelineFromFightLog,
  type AdapterResult,
} from '../src/lib/features/fight/timeline-from-fight-log.js';

// 002 RF-C1 and RF-C4 (plan-c decisions 1 to 3, step 2), with synthetic logs.
const ASSETS = { a: { asset: 'dummy_a' }, b: { asset: 'dummy_b' } } as const;

const a = validateManifest(dummyA, { expectedId: 'dummy_a' });
const b = validateManifest(dummyB, { expectedId: 'dummy_b' });
if (!a.ok || !b.ok) throw new Error('placeholder manifests do not validate');
const MANIFESTS = new Map([
  ['dummy_a', a.manifest],
  ['dummy_b', b.manifest],
]);

/** A KO with every variant: three kinds, hit, crit, block, dodge, knockdown, low_hp, moment. */
function koLog(): FightLog {
  return {
    ruleset_version: 1,
    a: { slug: 'striker', max_hp: 100 },
    b: { slug: 'tank', max_hp: 100 },
    events: [
      { type: 'intro', t: 0 },
      { type: 'attack', t: 2600, attacker: 'a', ability: 'strike', kind: 'strike' },
      {
        type: 'hit',
        t: 3000,
        attacker: 'a',
        ability: 'strike',
        damage: 10,
        crit: false,
        weak: true,
        hp: 90,
      },
      { type: 'attack', t: 3100, attacker: 'b', ability: 'slam', kind: 'heavy' },
      { type: 'block', t: 3800, attacker: 'b', ability: 'slam', damage: 5, hp: 95 },
      { type: 'attack', t: 3900, attacker: 'a', ability: 'burst', kind: 'special' },
      { type: 'dodge', t: 5000, attacker: 'a', ability: 'burst' },
      { type: 'attack', t: 5300, attacker: 'a', ability: 'strike', kind: 'strike' },
      {
        type: 'hit',
        t: 5700,
        attacker: 'a',
        ability: 'strike',
        damage: 40,
        crit: true,
        weak: false,
        hp: 50,
      },
      { type: 'knockdown', t: 5700, side: 'b', duration_ms: 1500 },
      { type: 'low_hp', t: 5700, side: 'b', hp: 50 },
      { type: 'attack', t: 7000, attacker: 'b', ability: 'strike', kind: 'strike' },
      {
        type: 'hit',
        t: 7400,
        attacker: 'b',
        ability: 'strike',
        damage: 20,
        crit: false,
        weak: false,
        hp: 75,
      },
      { type: 'attack', t: 7700, attacker: 'a', ability: 'slam', kind: 'heavy' },
      {
        type: 'hit',
        t: 8400,
        attacker: 'a',
        ability: 'slam',
        damage: 50,
        crit: false,
        weak: false,
        hp: 0,
      },
      { type: 'moment', t: 8400, kind: 'clutch_ko', side: 'a' },
      { type: 'ko', t: 8400, side: 'b' },
      { type: 'end', t: 12400, winner: 'a', reason: 'ko' },
    ],
  };
}

const EXPECTED: Timeline = {
  timeline_version: 1,
  duration_ms: 12400,
  fighters: { a: { asset: 'dummy_a', max_hp: 100 }, b: { asset: 'dummy_b', max_hp: 100 } },
  cues: [
    { at_ms: 0, type: 'intro' },
    {
      at_ms: 3000,
      type: 'strike',
      actor: 'a',
      move: 'attack_light',
      result: 'hit',
      damage: 10,
      target_hp: 90,
    },
    {
      at_ms: 3800,
      type: 'strike',
      actor: 'b',
      move: 'attack_heavy',
      result: 'block',
      damage: 5,
      target_hp: 95,
    },
    {
      at_ms: 5000,
      type: 'strike',
      actor: 'a',
      move: 'special',
      result: 'dodge',
      damage: 0,
      target_hp: 90,
    },
    {
      at_ms: 5700,
      type: 'strike',
      actor: 'a',
      move: 'attack_light',
      result: 'crit',
      damage: 40,
      target_hp: 50,
    },
    { at_ms: 5700, type: 'knockdown', fighter: 'b', duration_ms: 1500 },
    {
      at_ms: 7400,
      type: 'strike',
      actor: 'b',
      move: 'attack_light',
      result: 'hit',
      damage: 20,
      target_hp: 75,
    },
    {
      at_ms: 8400,
      type: 'strike',
      actor: 'a',
      move: 'attack_heavy',
      result: 'hit',
      damage: 50,
      target_hp: 0,
    },
    { at_ms: 8400, type: 'ko', fighter: 'b' },
    { at_ms: 9900, type: 'win', fighter: 'a' },
  ],
};

function withEvents(edit: (events: FightEvent[]) => void): FightLog {
  const log = koLog();
  edit(log.events);
  return log;
}

function codes(result: AdapterResult): string[] {
  return result.ok ? [] : result.errors.map((e) => e.code);
}

const at = (events: FightEvent[], type: FightEvent['type'], nth = 0): number => {
  const indexes = events.flatMap((e, i) => (e.type === type ? [i] : []));
  const index = indexes[nth < 0 ? indexes.length + nth : nth];
  if (index === undefined) throw new Error(`no ${type}`);
  return index;
};

describe('timelineFromFightLog', () => {
  it('maps a KO with every variant to exactly the v1 fields, and v1 accepts it', () => {
    const result = timelineFromFightLog(koLog(), ASSETS);
    expect(result).toEqual({
      ok: true,
      timeline: EXPECTED,
      ignored: { low_hp: 1, moment: 1, unknown: 0 },
    });
    expect(validateTimeline(EXPECTED, MANIFESTS).ok).toBe(true);
  });

  it('a KO by chip damage (a block down to 0, then ko) is ok and passes v1', () => {
    const log = withEvents((events) => {
      const last = at(events, 'hit', -1);
      events[last] = {
        type: 'block',
        t: 8400,
        attacker: 'a',
        ability: 'slam',
        damage: 50,
        hp: 0,
      };
    });
    const result = timelineFromFightLog(log, ASSETS);
    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.timeline.cues.at(-3)).toMatchObject({ result: 'block', target_hp: 0 });
    expect(validateTimeline(result.timeline, MANIFESTS).ok).toBe(true);
  });

  it('a dodge takes target_hp from the chain of HP', () => {
    const result = timelineFromFightLog(koLog(), ASSETS);
    if (!result.ok) throw new Error('not ok');
    const dodge = result.timeline.cues.find((c) => c.type === 'strike' && c.result === 'dodge');
    expect(dodge).toMatchObject({ damage: 0, target_hp: 90 });
  });

  it('low_hp, moment and an unknown type only count in `ignored`', () => {
    const log = withEvents((events) => {
      events.splice(3, 0, { type: 'taunt', t: 3000, side: 'a' } as unknown as FightEvent);
    });
    const result = timelineFromFightLog(log, ASSETS);
    expect(result).toEqual({
      ok: true,
      timeline: EXPECTED,
      ignored: { low_hp: 1, moment: 1, unknown: 1 },
    });
  });

  it.each<[string, (events: FightEvent[]) => void, string[]]>([
    ['intro_missing', (e) => e.shift(), ['adapter.intro_missing']],
    [
      'intro_missing (not at t = 0)',
      (e) => (e[0] = { type: 'intro', t: 5 }),
      ['adapter.intro_missing'],
    ],
    ['unsorted', (e) => (e[2] = { ...e[2], t: 100 } as FightEvent), ['adapter.unsorted']],
    ['resolution_without_attack', (e) => e.splice(1, 1), ['adapter.resolution_without_attack']],
    [
      'attack_mismatch (ability)',
      (e) => (e[2] = { ...e[2], ability: 'other' } as FightEvent),
      ['adapter.attack_mismatch'],
    ],
    [
      'attack_mismatch (attacker)',
      (e) => (e[6] = { ...e[6], attacker: 'b' } as FightEvent),
      ['adapter.attack_mismatch'],
    ],
    [
      'attack_without_resolution (attack after attack)',
      (e) =>
        e.splice(1, 0, { type: 'attack', t: 2500, attacker: 'b', ability: 'x', kind: 'strike' }),
      ['adapter.attack_without_resolution'],
    ],
    [
      'attack_without_resolution (before ko)',
      (e) => e.splice(at(e, 'hit', -1), 1),
      ['adapter.attack_without_resolution'],
    ],
    [
      'hp_mismatch (hp ≠ previous − damage)',
      (e) => (e[2] = { ...e[2], hp: 89 } as FightEvent),
      ['adapter.hp_mismatch'],
    ],
    [
      'hp_mismatch (damage over the previous hp)',
      (e) => (e[at(e, 'hit', -1)] = { ...e[at(e, 'hit', -1)], damage: 80, hp: 0 } as FightEvent),
      ['adapter.hp_mismatch'],
    ],
    ['end_missing', (e) => e.pop(), ['adapter.end_missing']],
    [
      'event_after_end',
      (e) => e.push({ type: 'low_hp', t: 13000, side: 'a', hp: 1 }),
      ['adapter.event_after_end'],
    ],
    [
      'unsupported_ending (time_up and decision)',
      (e) => {
        e.splice(at(e, 'ko'), 2);
        e.push(
          { type: 'time_up', t: 86000 },
          { type: 'end', t: 90000, winner: 'a', reason: 'decision' },
        );
      },
      ['adapter.unsupported_ending', 'adapter.unsupported_ending'],
    ],
    [
      'unsupported_ending (draw)',
      (e) => {
        e.splice(at(e, 'ko'), 2);
        e.push({ type: 'end', t: 90000, winner: null, reason: 'draw' });
      },
      ['adapter.unsupported_ending'],
    ],
    ['outcome_mismatch (no ko)', (e) => e.splice(at(e, 'ko'), 1), ['adapter.outcome_mismatch']],
    [
      'outcome_mismatch (two ko)',
      (e) => e.splice(at(e, 'ko'), 0, { type: 'ko', t: 8400, side: 'b' }),
      ['adapter.outcome_mismatch'],
    ],
    [
      'outcome_mismatch (winner is the side of the ko)',
      (e) => (e[at(e, 'end')] = { type: 'end', t: 12400, winner: 'b', reason: 'ko' }),
      ['adapter.outcome_mismatch'],
    ],
    [
      'outcome_mismatch (no winner)',
      (e) => (e[at(e, 'end')] = { type: 'end', t: 12400, winner: null, reason: 'ko' }),
      ['adapter.outcome_mismatch'],
    ],
  ])('%s', (_name, edit, expected) => {
    const result = timelineFromFightLog(withEvents(edit), ASSETS);
    expect(codes(result)).toEqual(expected);
    if (result.ok) return;
    for (const error of result.errors) expect(error.path).toMatch(/^\/events(?:\/\d+)?$/);
  });

  it('points at the event: /events/<i>', () => {
    const result = timelineFromFightLog(
      withEvents((e) => (e[2] = { ...e[2], hp: 89 } as FightEvent)),
      ASSETS,
    );
    expect(result.ok ? [] : result.errors).toEqual([
      { code: 'adapter.hp_mismatch', path: '/events/2', data: { expected: 90, actual: 89 } },
    ]);
  });

  it('stops at 100 errors plus adapter.too_many_errors (150 orphan resolutions)', () => {
    const log = withEvents((events) => {
      const orphans: FightEvent[] = Array.from({ length: 150 }, () => ({
        type: 'dodge',
        t: 2000,
        attacker: 'a',
        ability: 'strike',
      }));
      events.splice(1, 0, ...orphans);
    });
    const errors = timelineFromFightLog(log, ASSETS);
    expect(codes(errors)).toHaveLength(TIMELINE_LIMITS.maxErrors + 1);
    expect(codes(errors).slice(0, 100)).toEqual(
      Array(100).fill('adapter.resolution_without_attack'),
    );
    expect(codes(errors).at(-1)).toBe('adapter.too_many_errors');
  });

  it('WIN_DELAY_MS and timeline_version are the renderer values', () => {
    expect(WIN_DELAY_MS).toBe(TIMELINE_LIMITS.winGapMs);
    const result = timelineFromFightLog(koLog(), ASSETS);
    expect(result.ok && result.timeline.timeline_version).toBe(TIMELINE_VERSION);
  });

  it('the file uses neither clock nor randomness, and imports only types', () => {
    const source = readFileSync(
      new URL('../src/lib/features/fight/timeline-from-fight-log.ts', import.meta.url),
      'utf8',
    ).replace(/\/\/.*$/gm, '');
    expect(source).not.toMatch(/Math\.random|\bDate\b|\bperformance\b/);
    const imports = source.match(/^import .*$/gm) ?? [];
    expect(imports.length).toBeGreaterThan(0);
    for (const line of imports) expect(line).toMatch(/^import type /);
  });

  it('same input, same output', () => {
    const one = timelineFromFightLog(koLog(), ASSETS);
    const two = timelineFromFightLog(koLog(), ASSETS);
    expect(JSON.stringify(one)).toBe(JSON.stringify(two));
  });
});
