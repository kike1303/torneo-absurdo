import { describe, expect, it } from 'vitest';
import type { FightEvent, FightSummary } from '@torneo/protocol';
import { UP_TO_DATE_MS, createFightSession } from './fight-session.js';

// 010 plan-d decision 7. The pages have different sizes on purpose: nothing depends on them.
const HIT = (t: number, hp: number): FightEvent[] => [
  { type: 'attack', t: t - 400, attacker: 'a', ability: 'x', kind: 'strike' },
  { type: 'hit', t, attacker: 'a', ability: 'x', damage: 1, crit: false, weak: false, hp },
];
const INTRO: FightEvent = { type: 'intro', t: 0 };
const KO_AT = 5000;
const FULL: FightEvent[] = [
  INTRO,
  ...HIT(3000, 99),
  ...HIT(4000, 98),
  {
    type: 'hit',
    t: 4600,
    attacker: 'a',
    ability: 'x',
    damage: 98,
    crit: false,
    weak: false,
    hp: 0,
  },
];
const ENDED: FightEvent[] = [
  INTRO,
  { type: 'attack', t: KO_AT - 400, attacker: 'a', ability: 'x', kind: 'strike' },
  {
    type: 'hit',
    t: KO_AT,
    attacker: 'a',
    ability: 'x',
    damage: 100,
    crit: false,
    weak: false,
    hp: 0,
  },
  { type: 'ko', t: KO_AT, side: 'b' },
  { type: 'end', t: KO_AT + 4000, winner: 'a', reason: 'ko' },
];

function setup(over: Partial<FightSummary> = {}) {
  let events: FightEvent[] = [];
  let now = 1_000_000;
  let timer: (() => void) | null = null;
  let late = 0;
  let cleared = 0;
  let summary: FightSummary = {
    id: 1,
    state: 'live',
    a: { slug: 'a', max_hp: 100 },
    b: { slug: 'b', max_hp: 100 },
    ruleset_version: 1,
    snapshot_sha256: 'x',
    seed_commit: 'c',
    opens_at: '0',
    closes_at: '0',
    live_at: '0',
    live_started_at: String(now),
    time_limit_ms: 86_000,
    max_duration_ms: 90_000,
    playback_delay_ms: 3500,
    final: null,
    ...over,
  };
  const session = createFightSession({
    summary: () => summary,
    events: () => events,
    serverNow: () => now,
    assets: { a: 'dummy_a', b: 'dummy_b' },
    setTimer: (run) => {
      timer = run;
      return run;
    },
    clearTimer: () => {
      timer = null;
      cleared++;
    },
    onLate: () => late++,
  });
  return {
    session,
    set: (list: FightEvent[]) => {
      events = list;
      session.changed();
    },
    setSummary: (next: FightSummary) => {
      summary = next;
      session.changed();
    },
    quiet: () => {
      const run = timer;
      timer = null;
      run?.();
    },
    at: (value: number) => (now = value),
    hasTimer: () => timer !== null,
    late: () => late,
    cleared: () => cleared,
  };
}

const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));
async function resolved<T>(promise: Promise<T>): Promise<boolean> {
  const marker = Symbol();
  const winner = await Promise.race([
    promise.catch(() => undefined),
    flushMicrotasks().then(() => marker),
  ]);
  return winner !== marker;
}

describe('createFightSession: when it is ready', () => {
  it('without events it does not start: a fight_sync brings none', async () => {
    const { session, hasTimer } = setup();
    session.changed();
    expect(hasTimer()).toBe(false);
    expect(await resolved(session.ready)).toBe(false);
  });

  it('live before live_started_at: without the hour it waits, with the intro or not', () => {
    const { set, hasTimer } = setup({ live_started_at: null });
    set([INTRO]);
    expect(hasTimer()).toBe(false);
  });

  it('a first page without the intro does not start it', () => {
    const { set, hasTimer } = setup();
    set(HIT(3000, 99));
    expect(hasTimer()).toBe(false);
  });

  it(`with the intro it waits ${UP_TO_DATE_MS} ms without another page`, async () => {
    const { session, set, quiet, hasTimer, cleared } = setup();
    set([INTRO]);
    expect(hasTimer()).toBe(true);
    expect(await resolved(session.ready)).toBe(false);
    set(FULL.slice(0, 2));
    expect(cleared()).toBe(1);
    set(FULL.slice(0, 5));
    expect(cleared()).toBe(2);
    quiet();
    const live = await session.ready;
    expect(live.timeline.cues.map((c) => c.type)).toEqual(['intro', 'strike', 'strike']);
    expect(live.timeline.duration_ms).toBe(90_000);
  });

  it('with end it does not wait, and with end.t ahead it plays', async () => {
    const { session, set, late, at } = setup();
    at(1_000_000 + 3500 + 2000);
    set(ENDED);
    const live = await session.ready;
    expect(live.timeline.duration_ms).toBe(KO_AT + 4000);
    expect(late()).toBe(0);
  });

  it('with end.t gone by, nothing to play: no session, and it says so', async () => {
    const { session, set, late, at } = setup();
    at(1_000_000 + 3500 + KO_AT + 4000);
    set(ENDED);
    expect(late()).toBe(1);
    expect(await resolved(session.ready)).toBe(false);
  });

  it('live_started_at arriving later starts it', () => {
    const { set, setSummary, hasTimer } = setup({ live_started_at: null });
    set([INTRO]);
    setSummary({
      id: 1,
      live_started_at: '1000000',
      playback_delay_ms: 3500,
      max_duration_ms: 90_000,
      a: { slug: 'a', max_hp: 100 },
      b: { slug: 'b', max_hp: 100 },
    } as FightSummary);
    expect(hasTimer()).toBe(true);
  });

  it('events the adapter refuses reject it with timeline_invalid', async () => {
    const { session, set, quiet } = setup();
    set([INTRO, { type: 'dodge', t: 1, attacker: 'a', ability: 'x' }]);
    quiet();
    await expect(session.ready).rejects.toMatchObject({ code: 'timeline_invalid' });
  });
});

describe('createFightSession: once it is playing', () => {
  it('the clock is server now minus the start and the delay', async () => {
    const { session, set, quiet, at } = setup();
    set([INTRO]);
    quiet();
    const live = await session.ready;
    expect(live.clock()).toBe(-3500);
    at(1_000_000 + 3500 + 700);
    expect(live.clock()).toBe(700);
  });

  it('pushes each newer timeline, the pages of a tick together', async () => {
    const { session, set, quiet } = setup();
    set([INTRO]);
    quiet();
    const live = await session.ready;
    const pushed: number[] = [];
    live.subscribe((timeline) => pushed.push(timeline.cues.length));
    set(FULL.slice(0, 2));
    set(FULL.slice(0, 3));
    await flushMicrotasks();
    expect(pushed).toEqual([2]);
    set(FULL.slice(0, 5));
    await flushMicrotasks();
    expect(pushed).toEqual([2, 3]);
  });

  it('subscribing after end delivers the newest at once', async () => {
    const { session, set, quiet } = setup();
    set([INTRO]);
    quiet();
    const live = await session.ready;
    set(ENDED);
    await flushMicrotasks();
    const pushed: number[] = [];
    live.subscribe((timeline) => pushed.push(timeline.duration_ms));
    expect(pushed).toEqual([KO_AT + 4000]);
  });

  it('with nothing new, subscribing delivers nothing', async () => {
    const { session, set, quiet } = setup();
    set([INTRO]);
    quiet();
    const live = await session.ready;
    const pushed: number[] = [];
    live.subscribe((timeline) => pushed.push(timeline.cues.length));
    expect(pushed).toEqual([]);
  });

  it('a later page the adapter refuses calls fail, at once or later', async () => {
    const bad: FightEvent[] = [INTRO, { type: 'dodge', t: 1, attacker: 'a', ability: 'x' }];
    const later = setup();
    later.set([INTRO]);
    later.quiet();
    const live = await later.session.ready;
    const failed: string[] = [];
    live.subscribe(
      () => undefined,
      (code) => failed.push(code),
    );
    later.set(bad);
    await flushMicrotasks();
    expect(failed).toEqual(['timeline_invalid']);

    const atOnce = setup();
    atOnce.set([INTRO]);
    atOnce.quiet();
    const again = await atOnce.session.ready;
    atOnce.set(bad);
    await flushMicrotasks();
    const immediate: string[] = [];
    again.subscribe(
      () => undefined,
      (code) => immediate.push(code),
    );
    expect(immediate).toEqual(['timeline_invalid']);
  });

  it('after unsubscribing or stopping nothing arrives', async () => {
    const { session, set, quiet } = setup();
    set([INTRO]);
    quiet();
    const live = await session.ready;
    const pushed: number[] = [];
    const stop = live.subscribe((timeline) => pushed.push(timeline.cues.length));
    stop();
    set(FULL.slice(0, 3));
    await flushMicrotasks();
    live.subscribe((timeline) => pushed.push(timeline.cues.length));
    expect(pushed).toHaveLength(1);
    session.stop();
    set(FULL.slice(0, 5));
    await flushMicrotasks();
    expect(pushed).toHaveLength(1);
  });
});
