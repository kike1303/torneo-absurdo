// Choreography (spec "Diseño → Coreografía"): which animation each fighter plays and where,
// resolved once per `load` into segments, so `sceneAt` stays a pure lookup.
//
// Precedence, per fighter (the first rule that applies wins):
// 1. `ko` and `knockdown` are never interrupted.
// 2. Own windup: starts `windup_ms` before the impact and interrupts any reaction or own
//    recovery; if it starts before the return to the mark ends, the fighter stays at contact
//    distance (combo).
// 3. Impact received: the reaction interrupts the own recovery and the return.
// 4. Own recovery, then the return to the mark (≤ RETURN_MS).
// 5. Reaction in progress, until it ends.
// 6. `idle`.
// A windup that would start during an own `knockdown` waits for its end and starts at the frame
// that corresponds to `t − (at_ms − windup_ms)`, so the impact still lands on `at_ms`
// (plan-a1 decision 15).
import type { AnimationCode } from '../manifest/types.ts';
import {
  SIDES,
  otherSide,
  type Cue,
  type Side,
  type StrikeCue,
  type Timeline,
} from '../timeline/types.ts';
import { FACING, MARK_X } from './types.ts';

/** Return to the mark. */
export const RETURN_MS = 300;
/** Distance between the two pivots when a strike connects. */
export const CONTACT_PX = 64;
/** Cap for reactions and recoveries (the longest non-looping animation of the format). */
export const MAX_ACTION_MS = 3000;
/** Longest windup of the format: the renderer looks this far ahead (RF-A11). */
export const MAX_WINDUP_MS = 400;

export interface AnimationTiming {
  /** The animation actually played (after the RF-A5 fallback). */
  readonly code: AnimationCode;
  readonly frames: readonly number[];
  /** Cumulative end of each step, in ms from the animation start. */
  readonly ends: readonly number[];
  readonly total: number;
  readonly loop: boolean;
  readonly windup: number;
}

export type TimingTable = Readonly<Record<AnimationCode, AnimationTiming>>;

export type SegmentKind =
  'intro' | 'windup' | 'recovery' | 'reaction' | 'knockdown' | 'return' | 'contact';

/** A stretch of time in which one fighter plays one animation and moves linearly. */
export interface Segment {
  readonly side: Side;
  readonly kind: SegmentKind;
  readonly start: number;
  /** Exclusive. */
  readonly end: number;
  /** Requested code; `sceneAt` resolves it through the fighter's timing table. */
  readonly animation: AnimationCode;
  readonly animStart: number;
  readonly x0: number;
  readonly x1: number;
  readonly moveStart: number;
  readonly moveEnd: number;
  /** Index of the cue that caused it; `start ≥ at_ms − MAX_WINDUP_MS` and
   * `end ≤ at_ms + MAX_ACTION_MS + RETURN_MS`, so `cueWindow` always reaches it. */
  readonly owner: number;
}

/** `ko` and `win` hold their animation until the end of the timeline. */
export interface Hold {
  readonly start: number;
  readonly animation: AnimationCode;
  readonly x: number;
  readonly owner: number;
}

export interface Impact {
  readonly attackerX: number;
  readonly defenderX: number;
}

export interface Choreography {
  /** Segments by owner cue index. */
  readonly segments: readonly (readonly Segment[])[];
  readonly holds: Readonly<Record<Side, Hold | null>>;
  /** Positions at the impact, by strike cue index. */
  readonly impacts: readonly (Impact | undefined)[];
}

/** Step and manifest frame of an animation `elapsed` ms after it starts. */
export function frameAt(timing: AnimationTiming, elapsed: number): { step: number; frame: number } {
  const last = timing.frames.length - 1;
  let e = Math.max(0, elapsed);
  if (timing.loop) {
    e = e % timing.total;
  } else if (e >= timing.total) {
    return { step: last, frame: timing.frames[last] ?? 0 };
  }
  let step = 0;
  while (step < last && e >= (timing.ends[step] ?? 0)) step++;
  return { step, frame: timing.frames[step] ?? 0 };
}

export function xAt(segment: Segment, t: number): number {
  if (segment.moveEnd <= segment.moveStart || t >= segment.moveEnd) return segment.x1;
  if (t <= segment.moveStart) return segment.x0;
  const p = (t - segment.moveStart) / (segment.moveEnd - segment.moveStart);
  return segment.x0 + (segment.x1 - segment.x0) * p;
}

interface Open {
  kind: SegmentKind | 'idle';
  start: number;
  /** Natural end; Infinity when only an event can close it. */
  end: number;
  animation: AnimationCode;
  animStart: number;
  x0: number;
  x1: number;
  moveStart: number;
  moveEnd: number;
  owner: number;
}

interface Pending {
  cue: StrikeCue;
  index: number;
  windup: number;
}

interface Sim {
  side: Side;
  timing: TimingTable;
  cur: Open;
  hold: Hold | null;
  pending: Pending | null;
  /** Start of every own windup, sorted. */
  windups: number[];
  out: Segment[][];
}

const MOVE_EPSILON = 0.5;

function curX(sim: Sim, t: number): number {
  const c = sim.cur;
  return xAt({ ...c, side: sim.side, kind: 'return' }, t);
}

function close(sim: Sim, t: number): void {
  const c = sim.cur;
  if (c.kind === 'idle' || t <= c.start) return;
  const bucket = sim.out[c.owner];
  if (bucket === undefined) return;
  bucket.push({
    side: sim.side,
    kind: c.kind,
    start: c.start,
    end: t,
    animation: c.animation,
    animStart: c.animStart,
    x0: c.x0,
    x1: c.x1,
    moveStart: c.moveStart,
    moveEnd: c.moveEnd,
    owner: c.owner,
  });
}

function still(
  kind: Open['kind'],
  start: number,
  end: number,
  animation: AnimationCode,
  x: number,
  owner: number,
): Open {
  return {
    kind,
    start,
    end,
    animation,
    animStart: start,
    x0: x,
    x1: x,
    moveStart: start,
    moveEnd: start,
    owner,
  };
}

function idle(sim: Sim, t: number): Open {
  return still('idle', t, Number.POSITIVE_INFINITY, 'idle', MARK_X[sim.side], -1);
}

/** Natural end of a reaction, recovery, intro or knockdown at `e`, with the fighter at `x`. */
function afterAction(sim: Sim, e: number, x: number, owner: number): Open {
  const mark = MARK_X[sim.side];
  if (Math.abs(x - mark) < MOVE_EPSILON) return idle(sim, e);
  const next = sim.windups.find((w) => w >= e);
  if (next !== undefined && next < e + RETURN_MS) {
    // Rule 2: the next windup starts before the return would end, so stay at contact.
    return still('contact', e, Number.POSITIVE_INFINITY, 'idle', x, owner);
  }
  return {
    kind: 'return',
    start: e,
    end: e + RETURN_MS,
    animation: 'walk',
    animStart: e,
    x0: x,
    x1: mark,
    moveStart: e,
    moveEnd: e + RETURN_MS,
    owner,
  };
}

function windupSegment(sim: Sim, p: Pending, start: number, x: number): Open {
  return {
    kind: 'windup',
    start,
    end: Number.POSITIVE_INFINITY,
    animation: p.cue.move,
    animStart: p.cue.at_ms - p.windup,
    x0: x,
    x1: x,
    moveStart: start,
    moveEnd: p.cue.at_ms,
    owner: p.index,
  };
}

function advance(sim: Sim, time: number): void {
  while (sim.hold === null && Number.isFinite(sim.cur.end) && sim.cur.end <= time) {
    const c = sim.cur;
    const e = c.end;
    const x = curX(sim, e);
    close(sim, e);
    if (c.kind === 'return') {
      sim.cur = idle(sim, e);
    } else if (c.kind === 'knockdown' && sim.pending !== null) {
      sim.cur = windupSegment(sim, sim.pending, e, x);
      sim.pending = null;
    } else {
      sim.cur = afterAction(sim, e, x, c.owner);
    }
  }
}

function actionMs(timing: AnimationTiming): number {
  return Math.min(timing.total, MAX_ACTION_MS);
}

type EventKind = 'intro' | 'impact' | 'knockdown' | 'ko' | 'win' | 'windup';
const PRIORITY: Readonly<Record<EventKind, number>> = {
  intro: 0,
  impact: 1,
  knockdown: 2,
  ko: 3,
  win: 4,
  windup: 5,
};

interface SimEvent {
  time: number;
  kind: EventKind;
  index: number;
  cue: Cue;
}

export function buildChoreography(
  timeline: Timeline,
  timings: Readonly<Record<Side, TimingTable>>,
): Choreography {
  const out: Segment[][] = timeline.cues.map(() => []);
  const impacts: (Impact | undefined)[] = timeline.cues.map(() => undefined);
  const sims: Record<Side, Sim> = { a: makeSim('a'), b: makeSim('b') };

  function makeSim(side: Side): Sim {
    return {
      side,
      timing: timings[side],
      cur: still('idle', 0, Number.POSITIVE_INFINITY, 'idle', MARK_X[side], -1),
      hold: null,
      pending: null,
      windups: [],
      out,
    };
  }

  const events: SimEvent[] = [];
  timeline.cues.forEach((cue, index) => {
    switch (cue.type) {
      case 'intro':
        events.push({ time: cue.at_ms, kind: 'intro', index, cue });
        break;
      case 'strike': {
        const windup = timings[cue.actor][cue.move].windup;
        sims[cue.actor].windups.push(cue.at_ms - windup);
        events.push({ time: cue.at_ms - windup, kind: 'windup', index, cue });
        events.push({ time: cue.at_ms, kind: 'impact', index, cue });
        break;
      }
      case 'knockdown':
        events.push({ time: cue.at_ms, kind: 'knockdown', index, cue });
        break;
      case 'ko':
        events.push({ time: cue.at_ms, kind: 'ko', index, cue });
        break;
      case 'win':
        events.push({ time: cue.at_ms, kind: 'win', index, cue });
        break;
    }
  });
  events.sort(
    (p, q) => p.time - q.time || PRIORITY[p.kind] - PRIORITY[q.kind] || p.index - q.index,
  );
  for (const side of SIDES) sims[side].windups.sort((p, q) => p - q);

  for (const ev of events) {
    for (const side of SIDES) advance(sims[side], ev.time);
    const { cue, index, time } = ev;

    if (cue.type === 'intro') {
      for (const side of SIDES) {
        const sim = sims[side];
        const timing = sim.timing.intro;
        if (timing.code === 'intro') {
          close(sim, time);
          sim.cur = still('intro', time, time + actionMs(timing), 'intro', curX(sim, time), index);
        }
      }
    } else if (cue.type === 'strike') {
      const attacker = sims[cue.actor];
      const defender = sims[otherSide(cue.actor)];
      if (ev.kind === 'windup') {
        if (attacker.hold !== null) continue;
        const pending = { cue, index, windup: attacker.timing[cue.move].windup };
        if (attacker.cur.kind === 'knockdown') {
          attacker.pending = pending;
        } else {
          const x = curX(attacker, time);
          close(attacker, time);
          attacker.cur = windupSegment(attacker, pending, time, x);
        }
        continue;
      }
      // Impact.
      const defenderX = defender.hold !== null ? defender.hold.x : curX(defender, time);
      let attackerX = attacker.hold !== null ? attacker.hold.x : curX(attacker, time);
      const a = attacker.cur;
      if (attacker.hold === null && a.kind === 'windup' && a.owner === index) {
        const dir = FACING[cue.actor];
        const contact = defenderX - dir * CONTACT_PX;
        // Never step away from the rival: stay put if already closer than contact distance.
        a.x1 = dir > 0 ? Math.max(a.x0, contact) : Math.min(a.x0, contact);
        attackerX = a.x1;
        close(attacker, time);
        const timing = attacker.timing[cue.move];
        const recovery = Math.min(timing.total - timing.windup, MAX_ACTION_MS);
        attacker.cur = {
          ...still('recovery', time, time + recovery, cue.move, attackerX, index),
          animStart: a.animStart,
        };
      }
      impacts[index] = { attackerX, defenderX };
      if (defender.hold === null && defender.cur.kind !== 'knockdown') {
        const reaction: AnimationCode =
          cue.result === 'block' ? 'block' : cue.result === 'dodge' ? 'dodge' : 'hit';
        close(defender, time);
        defender.cur = still(
          'reaction',
          time,
          time + actionMs(defender.timing[reaction]),
          reaction,
          defenderX,
          index,
        );
      }
    } else if (cue.type === 'knockdown') {
      const sim = sims[cue.fighter];
      if (sim.hold !== null) continue;
      const x = curX(sim, time);
      close(sim, time);
      sim.cur = still('knockdown', time, time + cue.duration_ms, 'knockdown', x, index);
    } else {
      // ko or win: hold until the end.
      const sim = sims[cue.fighter];
      if (sim.hold !== null) continue;
      const x = curX(sim, time);
      close(sim, time);
      sim.hold = { start: time, animation: cue.type, x, owner: index };
      sim.cur = idle(sim, time);
    }
  }

  for (const side of SIDES) {
    const sim = sims[side];
    advance(sim, Number.POSITIVE_INFINITY);
    if (sim.hold === null && Number.isFinite(sim.cur.end)) close(sim, sim.cur.end);
  }

  return {
    segments: out,
    holds: { a: sims.a.hold, b: sims.b.hold },
    impacts,
  };
}
