import { describe, expect, it } from 'vitest';
import type { FightFinal, FightSummary, FightsCurrent } from '@torneo/protocol';
import {
  POSE_AFTER_END_MS,
  adoptsCurrent,
  viewState,
  watchedFinished,
  type ViewInput,
  type Watched,
} from './view-state.js';

// 010 RF54 and plan-d decision 4: a row per case.
const NOW = 1_000_000;
const DELAY = 3500;
const MAX = 90_000;

function fight(id: number, over: Partial<FightSummary> = {}): FightSummary {
  return {
    id,
    state: 'scheduled',
    a: { slug: 'a', max_hp: 100 },
    b: { slug: 'b', max_hp: 100 },
    ruleset_version: 1,
    snapshot_sha256: 'x',
    seed_commit: null,
    opens_at: '0',
    closes_at: '0',
    live_at: String(NOW + 60_000),
    live_started_at: null,
    time_limit_ms: 86_000,
    max_duration_ms: MAX,
    playback_delay_ms: DELAY,
    final: null,
    ...over,
  };
}

const started = (t: number) => String(NOW - DELAY - t);
const liveFight = (id: number, t: number, over: Partial<FightSummary> = {}) =>
  fight(id, { state: 'live', live_started_at: started(t), ...over });
const settled = (id: number, final: FightFinal, t = 0) =>
  fight(id, { state: 'settled', live_started_at: started(t), final });
const KO: FightFinal = { kind: 'decided', winner: 'a', reason: 'ko' };
const DRAW: FightFinal = { kind: 'decided', winner: null, reason: 'draw' };
const VOIDED: FightFinal = { kind: 'voided', reason: 'engine_error' };
const CANCELLED: FightFinal = { kind: 'cancelled', reason: 'missed_window' };

function board(over: Partial<FightsCurrent> = {}): FightsCurrent {
  return {
    server_time: String(NOW),
    halted: false,
    current: null,
    next: null,
    last: null,
    ...over,
  };
}

function watched(summary: FightSummary, t: number | null, over: Partial<Watched> = {}): Watched {
  return {
    summary,
    t,
    end: null,
    ending: null,
    painted: true,
    textMode: false,
    lateNoPixi: false,
    pending: true,
    ...over,
  };
}

function view(over: Partial<ViewInput>) {
  return viewState({
    board: board(),
    watched: null,
    serverNow: NOW,
    degraded: false,
    ssrFailed: false,
    ...over,
  });
}

describe('viewState: the rows of the table', () => {
  it('1. the SSR failed and there is no data: error', () => {
    expect(view({ board: null, ssrFailed: true }).kind).toBe('error');
  });

  it('1. the SSR failed but data arrived: not an error', () => {
    expect(view({ ssrFailed: true }).kind).toBe('starting');
  });

  it('2. halted true wins over live and about to start, and keeps last', () => {
    const last = settled(1, KO);
    const current = liveFight(2, 1000);
    const halted = view({ board: board({ halted: true, current, last }) });
    expect(halted.kind).toBe('halted');
    expect(halted.result?.fight.id).toBe(1);
    expect(view({ board: board({ halted: true, current: fight(2) }) }).kind).toBe('halted');
  });

  it('2. halted null never shows halted', () => {
    expect(view({ board: board({ halted: null, current: fight(1) }) }).kind).toBe('next');
  });

  it('2. halted with the end already seen keeps the result from the events', () => {
    const summary = liveFight(2, 20_000);
    const w = watched(summary, 20_000, {
      end: { t: 15_000, winner: 'b', reason: 'ko' },
      pending: false,
    });
    const v = view({ board: board({ halted: true, current: summary }), watched: w });
    expect(v.kind).toBe('halted');
    expect(v.result).toMatchObject({ kind: 'ko', winner: 'b' });
  });

  it('2. halted unmounts the canvas with nothing left to play, and keeps it if there is', () => {
    const summary = liveFight(2, 5000);
    const b = board({ halted: true, current: summary });
    expect(view({ board: b, watched: watched(summary, 5000, { pending: true }) }).stage).toBe(
      'visible',
    );
    expect(view({ board: b, watched: watched(summary, 5000, { pending: false }) }).stage).toBe(
      'none',
    );
  });

  it('3. all null and halted null: loading', () => {
    expect(view({ board: board({ halted: null }) }).kind).toBe('loading');
  });

  it('4. all null and halted false: starting', () => {
    expect(view({}).kind).toBe('starting');
  });

  it('5. a watched fight not finished, painted and with t ≥ 0: live, with the canvas', () => {
    const summary = liveFight(2, 1000);
    const v = view({ board: board({ current: summary }), watched: watched(summary, 1000) });
    expect(v).toMatchObject({
      kind: 'live',
      stage: 'visible',
      liveBadge: true,
      showRoundClock: true,
    });
    expect(v.live?.id).toBe(2);
  });

  it('5. the label goes as soon as there is an ending, and the pose stays', () => {
    const summary = liveFight(2, 12_000);
    const w = watched(summary, 12_000, { ending: { winner: 'a', reason: 'ko' } });
    expect(view({ board: board({ current: summary }), watched: w })).toMatchObject({
      kind: 'live',
      stage: 'visible',
      liveBadge: false,
    });
  });

  it('6. a live fight without a session: live without canvas', () => {
    const current = liveFight(2, 1000);
    expect(view({ board: board({ current }) })).toMatchObject({
      kind: 'live',
      stage: 'none',
      liveBadge: true,
    });
  });

  it('6. a watched fight waiting for its first frame: about to start, canvas hidden', () => {
    const current = liveFight(2, 1000);
    const v = view({
      board: board({ current }),
      watched: watched(current, 1000, { painted: false }),
    });
    expect(v).toMatchObject({ kind: 'about_to_start', stage: 'hidden' });
    expect(v.upcoming?.id).toBe(2);
  });

  it('7. the target went by without live: about to start, never a frozen 0:00, and last stays', () => {
    const last = settled(1, KO);
    const current = fight(2, { state: 'betting_closed', live_at: String(NOW - DELAY - 1) });
    const v = view({ board: board({ current, last }) });
    expect(v.kind).toBe('about_to_start');
    expect(v.result?.fight.id).toBe(1);
  });

  it('8. a fight to come: next, with last', () => {
    const last = settled(1, DRAW);
    const v = view({ board: board({ current: fight(2), last }) });
    expect(v.kind).toBe('next');
    expect(v.upcoming?.id).toBe(2);
    expect(v.target).toBe(NOW + 60_000 + DELAY);
    expect(v.result).toMatchObject({ kind: 'draw', winner: null });
  });

  it('8. the target follows live_started_at once it is known', () => {
    const current = fight(2, {
      state: 'live',
      live_started_at: String(NOW + 10_000),
      live_at: String(NOW - 50_000),
    });
    expect(view({ board: board({ current }) })).toMatchObject({
      kind: 'next',
      target: NOW + 10_000 + DELAY,
    });
  });

  it('8. pre_live: the board stays, with the canvas mounted and hidden', () => {
    const current = fight(2, { state: 'betting_closed' });
    const v = view({
      board: board({ current }),
      watched: watched(current, null, { painted: false }),
    });
    expect(v).toMatchObject({ kind: 'next', stage: 'hidden' });
  });

  it('9. only last: the result', () => {
    expect(view({ board: board({ last: settled(1, KO) }) })).toMatchObject({
      kind: 'result',
      result: { kind: 'ko', winner: 'a' },
    });
  });

  it('9. a cancelled last: "did not take place"', () => {
    expect(view({ board: board({ last: settled(1, CANCELLED) }) }).result?.kind).toBe('cancelled');
  });
});

// The review of PR #82: the fight being watched is not left until it is finished.
describe('adoptsCurrent', () => {
  const closed = fight(3, { state: 'betting_closed' });
  const ko = { t: 55_123, winner: 'a' as const, reason: 'ko' as const };

  it('with nothing watched, current from pre_live on', () => {
    expect(adoptsCurrent(closed, null)).toBe(true);
    expect(adoptsCurrent(liveFight(3, 0), null)).toBe(true);
    expect(adoptsCurrent(fight(3), null)).toBe(false);
    expect(adoptsCurrent(fight(3, { state: 'betting_open' }), null)).toBe(false);
    expect(adoptsCurrent(null, null)).toBe(false);
  });

  it('the fight watched is not adopted again', () => {
    const summary = liveFight(3, 1000);
    expect(adoptsCurrent(summary, watched(summary, 1000))).toBe(false);
  });

  it('the pose in course keeps it, with the next fight in pre_live', () => {
    const summary = settled(2, KO, 56_736);
    const w = watched(summary, 56_736, { end: ko, ending: ko, pending: false });
    expect(adoptsCurrent(closed, w)).toBe(false);
  });

  it('a late client far from its KO keeps it, and its final does not show', () => {
    const summary = settled(2, KO, 12_403);
    const w = watched(summary, 12_403);
    expect(adoptsCurrent(closed, w)).toBe(false);
    const v = view({ board: board({ current: closed, last: summary }), watched: w });
    expect(v).toMatchObject({ kind: 'live', stage: 'visible', result: null });
  });

  it('the pose over: the next fight', () => {
    const summary = settled(2, KO, ko.t + POSE_AFTER_END_MS);
    const w = watched(summary, ko.t + POSE_AFTER_END_MS, { end: ko, ending: ko, pending: false });
    expect(adoptsCurrent(closed, w)).toBe(true);
  });

  it('a voided fight is over at once', () => {
    const summary = settled(2, VOIDED, 5000);
    expect(adoptsCurrent(closed, watched(summary, 5000))).toBe(true);
  });

  it('a fight the board no longer names does not hold the next one forever', () => {
    const summary = liveFight(2, MAX + POSE_AFTER_END_MS);
    expect(adoptsCurrent(closed, watched(summary, MAX + POSE_AFTER_END_MS))).toBe(true);
    expect(adoptsCurrent(closed, watched(summary, MAX + POSE_AFTER_END_MS - 1))).toBe(false);
  });

  it('one that never started does not hold it either', () => {
    expect(adoptsCurrent(closed, watched(fight(2, { state: 'betting_closed' }), null))).toBe(true);
  });
});

describe('viewState: the cases of the review', () => {
  it('coming in past end.t with its first frame pending: the fight is over, no "about to start"', () => {
    const summary = liveFight(2, 56_000);
    const w = watched(summary, 56_000, {
      end: { t: 54_000, winner: 'a', reason: 'ko' },
      painted: false,
      pending: false,
    });
    expect(watchedFinished(w)).toBe(true);
    expect(view({ board: board({ current: summary }), watched: w })).toMatchObject({
      kind: 'result',
      stage: 'none',
    });
  });

  it('a canvas already painted when a late end arrives keeps its pose', () => {
    const summary = liveFight(2, 56_000);
    const w = watched(summary, 56_000, {
      end: { t: 54_000, winner: 'a', reason: 'ko' },
      pending: false,
    });
    expect(watchedFinished(w)).toBe(false);
  });

  it('halted in text mode keeps the fight in view', () => {
    const summary = liveFight(2, 5000);
    const w = watched(summary, 5000, { textMode: true });
    const v = view({ board: board({ halted: true, current: summary }), watched: w });
    expect(v).toMatchObject({ kind: 'halted', stage: 'none' });
    expect(v.live?.id).toBe(2);
  });

  it('current live with t < 0, without next nor last: its countdown, not an empty view', () => {
    const current = fight(2, { state: 'live', live_started_at: String(NOW - 1000) });
    const v = view({ board: board({ current }) });
    expect(v.kind).toBe('next');
    expect(v.upcoming?.id).toBe(2);
    expect(v.target).toBe(NOW - 1000 + DELAY);
  });

  it('current live with t < 0 is what comes, not the next one', () => {
    const current = fight(2, { state: 'live', live_started_at: String(NOW - 1000) });
    expect(view({ board: board({ current, next: fight(3) }) }).upcoming?.id).toBe(2);
  });

  it('Settled during the pose: the pose stays', () => {
    const summary = settled(2, KO, 58_000);
    const w = watched(summary, 58_000, {
      end: { t: 54_000, winner: 'a', reason: 'ko' },
      ending: { winner: 'a', reason: 'ko' },
      pending: false,
    });
    const v = view({ board: board({ current: fight(3), last: summary }), watched: w });
    expect(v).toMatchObject({ kind: 'live', stage: 'visible', liveBadge: false });
  });

  it('a draw with Settled during the pose: 10 s of pose, not 1.5', () => {
    const summary = settled(2, DRAW, MAX + 1500);
    const w = watched(summary, MAX + 1500, {
      end: { t: MAX, winner: null, reason: 'draw' },
      ending: { winner: null, reason: 'draw' },
      pending: false,
    });
    expect(view({ board: board({ last: summary }), watched: w }).kind).toBe('live');
    const after = { ...w, t: MAX + POSE_AFTER_END_MS };
    expect(view({ board: board({ last: summary }), watched: after }).kind).toBe('result');
  });

  it('log-ko-clutch (end.t = 86 866) at t = 90 000: the pose is not cut', () => {
    const summary = settled(2, KO, MAX);
    const w = watched(summary, MAX, {
      end: { t: 86_866, winner: 'a', reason: 'ko' },
      ending: { winner: 'a', reason: 'ko' },
      pending: false,
    });
    expect(view({ board: board({ last: summary }), watched: w }).kind).toBe('live');
  });

  it('a late client: a decided final does not give the result before t ≥ max_duration_ms', () => {
    const summary = settled(2, KO, 20_000);
    const w = watched(summary, 20_000);
    const v = view({ board: board({ last: summary, current: fight(3) }), watched: w });
    expect(v.kind).toBe('live');
    expect(v.result).toBeNull();
  });

  it('a watched fight without end: a decided final shows from t ≥ max_duration_ms', () => {
    const summary = settled(2, KO, MAX);
    const w = watched(summary, MAX, { pending: false });
    const v = view({ board: board({ last: summary }), watched: w });
    expect(v).toMatchObject({ kind: 'result', result: { kind: 'ko' } });
  });

  it('a watched fight that becomes voided: at once, and not live forever', () => {
    const summary = settled(2, VOIDED, 5000);
    const w = watched(summary, 5000);
    expect(view({ board: board({ last: summary }), watched: w })).toMatchObject({
      kind: 'result',
      stage: 'none',
      result: { kind: 'voided' },
    });
  });

  it('a watched fight left without events by ws.forbidden_channel: same rule', () => {
    const summary = settled(2, KO, MAX + 1);
    const w = watched(summary, MAX + 1, { painted: false, pending: false });
    expect(view({ board: board({ last: summary }), watched: w }).result?.kind).toBe('ko');
  });

  it('the pose ended with the settlement late: the result from the events, no countdown', () => {
    const summary = liveFight(2, 70_000);
    const w = watched(summary, 70_000, {
      end: { t: 54_000, winner: 'b', reason: 'ko' },
      ending: { winner: 'b', reason: 'ko' },
      pending: false,
    });
    const alone = view({ board: board({ current: summary }), watched: w });
    expect(alone).toMatchObject({
      kind: 'result',
      stage: 'none',
      result: { kind: 'ko', winner: 'b' },
    });
    const withNext = view({ board: board({ current: summary, next: fight(3) }), watched: w });
    expect(withNext.kind).toBe('next');
    expect(withNext.upcoming?.id).toBe(3);
    expect(withNext.result?.fight.id).toBe(2);
  });

  it('coming in with end.t gone by and current live: no Pixi, the result', () => {
    const summary = liveFight(2, 56_000);
    const w = watched(summary, 56_000, {
      end: { t: 54_000, winner: 'a', reason: 'ko' },
      lateNoPixi: true,
      painted: false,
      pending: false,
    });
    expect(view({ board: board({ current: summary }), watched: w })).toMatchObject({
      kind: 'result',
      stage: 'none',
      result: { kind: 'ko', winner: 'a' },
    });
  });

  it('text mode counts as painted: live, without canvas', () => {
    const summary = liveFight(2, 1000);
    const w = watched(summary, 1000, { textMode: true, painted: true });
    expect(view({ board: board({ current: summary }), watched: w })).toMatchObject({
      kind: 'live',
      stage: 'none',
    });
  });

  it('text mode: the result only from t ≥ end.t', () => {
    const summary = liveFight(2, 53_999);
    const end = { t: 54_000, winner: 'a' as const, reason: 'ko' as const };
    const before = watched(summary, 53_999, { textMode: true, end });
    expect(view({ board: board({ current: summary }), watched: before }).result).toBeNull();
    const after = { ...before, t: 54_000 };
    expect(view({ board: board({ current: summary }), watched: after }).result?.kind).toBe('ko');
  });

  it('last stays in pre_live, until the next fight is live', () => {
    const last = settled(1, KO);
    const closed = fight(2, { state: 'betting_closed' });
    const before = view({
      board: board({ current: closed, last }),
      watched: watched(closed, null, { painted: false }),
    });
    expect(before.result?.fight.id).toBe(1);
    const summary = liveFight(2, 1000);
    const during = view({
      board: board({ current: summary, last }),
      watched: watched(summary, 1000),
    });
    expect(during.result).toBeNull();
    expect(view({ board: board({ current: summary, last }) }).result).toBeNull();
  });

  it('degraded, a watched fight waiting for its first frame is live, without a canvas in view', () => {
    const current = liveFight(2, 1000);
    const v = view({
      board: board({ current }),
      watched: watched(current, 1000, { painted: false }),
      degraded: true,
    });
    expect(v).toMatchObject({ kind: 'live', showRoundClock: false });
    expect(v.stage).not.toBe('visible');
  });

  it('degraded adds no state, and hides the round clock', () => {
    const current = liveFight(2, 1000);
    expect(view({ board: board({ current }), degraded: true })).toMatchObject({
      kind: 'live',
      showRoundClock: false,
    });
    expect(view({ board: board({ current: fight(2) }), degraded: true }).kind).toBe('next');
  });
});
