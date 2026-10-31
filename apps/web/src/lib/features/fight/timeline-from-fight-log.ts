// The narrow adapter from the engine log to the renderer timeline v2 (002 RF-C1, plan-c decisions
// 1 to 3; the endings by time in 005 RF11 to RF13; the prefixes of a fight in 010 RF51). The only
// file that knows both shapes
// (tests/fight-boundary.test.ts). Pure: no I/O, no clock and no randomness; codes only, no text
// (ADR-0007). It never fixes nor invents anything (RF-C4): what v2 can check is left to
// `validateTimeline` in `load`.
import type { AbilityKind, FightEvent, FightLog, Side } from '@torneo/protocol';
import type { Cue, Move, Timeline } from '@torneo/renderer/timeline';

export type AdapterErrorCode = `adapter.${string}`;

export interface AdapterError {
  readonly code: AdapterErrorCode;
  /** JSON pointer into the log, e.g. "/events/12". */
  readonly path: string;
  readonly data?: Readonly<Record<string, string | number>>;
}

export type IgnoredCounts = Readonly<Record<'low_hp' | 'moment' | 'unknown', number>>;

export type AdapterResult =
  | { readonly ok: true; readonly timeline: Timeline; readonly ignored: IgnoredCounts }
  | { readonly ok: false; readonly errors: readonly AdapterError[] };

/** The timeline the adapter produces, for callers that must not import the renderer types too. */
export type AdaptedTimeline = Extract<AdapterResult, { ok: true }>['timeline'];

/** The log the adapter reads, for callers that must not import the protocol types too. */
export type AdapterLog = FightLog;

/** The events the adapter reads, for callers that must not import the protocol types too. */
export type AdapterEvent = FightEvent;

/**
 * The header of a fight in progress, the least the adapter needs: `FightLog` and `FightSummary`
 * both fit it by structure (010 RF51).
 */
export type LiveHeader = Readonly<Record<Side, { readonly max_hp: number }>>;

type AdapterFighters = Readonly<Record<Side, { readonly asset: string }>>;

/** Gap between `ko` and `win` (equal to `TIMELINE_LIMITS.winGapMs`, checked by a test). */
export const WIN_DELAY_MS = 1500;
/**
 * Gap between `time_up` and its closing, `win` (decision) or `draw` (005 spec P2; equal to
 * `TIMELINE_LIMITS.winGapMs`, checked by a test).
 */
export const CLOSING_DELAY_MS = 1500;
/** Equal to `TIMELINE_VERSION` (checked by a test): only types may be imported here. */
const VERSION = 2;
const MAX_ERRORS = 100;

const MOVE_BY_KIND: Readonly<Record<AbilityKind, Move>> = {
  strike: 'attack_light',
  heavy: 'attack_heavy',
  special: 'special',
};

const KNOWN_TYPES: ReadonlySet<string> = new Set<FightEvent['type']>([
  'intro',
  'attack',
  'hit',
  'block',
  'dodge',
  'knockdown',
  'low_hp',
  'moment',
  'ko',
  'time_up',
  'end',
]);

const other = (side: Side): Side => (side === 'a' ? 'b' : 'a');

interface Pending {
  readonly attacker: Side;
  readonly ability: string;
  readonly move: Move;
  readonly index: number;
}

export function timelineFromFightLog(log: FightLog, fighters: AdapterFighters): AdapterResult {
  return adaptEvents(log.events, log, fighters, null);
}

/**
 * The timeline of the events released so far (010 RF51): an `attack` still unresolved at the end
 * and the lack of `end` are not errors. Until `end` arrives, `duration_ms` is `maxDurationMs`; from
 * then on, `end.t` (RF48). The first event must be the `intro`, so the host waits for it.
 */
export function liveTimelineFromEvents(
  header: LiveHeader,
  events: readonly FightEvent[],
  fighters: AdapterFighters,
  maxDurationMs: number,
): AdapterResult {
  return adaptEvents(events, header, fighters, maxDurationMs);
}

/** The one core: with `liveMaxDurationMs`, `events` is a prefix of a fight; if null, all of it. */
function adaptEvents(
  events: readonly FightEvent[],
  header: LiveHeader,
  fighters: AdapterFighters,
  liveMaxDurationMs: number | null,
): AdapterResult {
  const errors: AdapterError[] = [];
  let full = false;
  const fail = (code: AdapterErrorCode, path: string, data?: AdapterError['data']): void => {
    if (full) return;
    if (errors.length >= MAX_ERRORS) {
      errors.push({ code: 'adapter.too_many_errors', path });
      full = true;
      return;
    }
    errors.push(data === undefined ? { code, path } : { code, path, data });
  };

  const first = events[0];
  if (first === undefined || first.type !== 'intro' || first.t !== 0)
    fail('adapter.intro_missing', '/events/0');

  const hp: Record<Side, number> = { a: header.a.max_hp, b: header.b.max_hp };
  // After a bad resolution the HP is uncertain: `hp` follows the log and `alt` the other reading
  // (the chain without the bad value, or without the orphan event). The next hit or block on that
  // side accepts either and settles it, so one bad event gives one error.
  const alt: Record<Side, number | null> = { a: null, b: null };
  const ignored = { low_hp: 0, moment: 0, unknown: 0 };
  const cues: Cue[] = [];
  const kos: { readonly side: Side; readonly t: number }[] = [];
  const timeUps: { readonly t: number }[] = [];
  let pending: Pending | null = null;
  let end: { readonly t: number; readonly winner: Side | null } | null = null;
  let previousT = 0;

  const unresolved = (): void => {
    if (pending === null) return;
    fail('adapter.attack_without_resolution', `/events/${pending.index}`);
    pending = null;
  };

  for (const [index, event] of events.entries()) {
    const path = `/events/${index}`;
    if (end !== null) {
      fail('adapter.event_after_end', path);
      continue;
    }
    const t: unknown = event.t;
    if (typeof t !== 'number' || !Number.isFinite(t)) {
      // Out of the ordering chain, so it never hides an `unsorted` after it.
      if (KNOWN_TYPES.has(event.type)) fail('adapter.invalid_t', path);
      else ignored.unknown++;
      continue;
    }
    if (t < previousT) fail('adapter.unsorted', path);
    // It would be a cue the playback can never reach.
    if (liveMaxDurationMs !== null && t > liveMaxDurationMs)
      fail('adapter.past_max_duration', path, { t, max: liveMaxDurationMs });
    previousT = Math.max(previousT, t);

    switch (event.type) {
      case 'intro':
        cues.push({ at_ms: event.t, type: 'intro' });
        continue;
      case 'attack':
        unresolved();
        pending = {
          attacker: event.attacker,
          ability: event.ability,
          move: MOVE_BY_KIND[event.kind],
          index,
        };
        continue;
      case 'hit':
      case 'block':
      case 'dodge': {
        const target = other(event.attacker);
        const damage = event.type === 'dodge' ? 0 : event.damage;
        const reported = event.type === 'dodge' ? hp[target] : event.hp;
        const second = alt[target];
        const before =
          event.type !== 'dodge' && reported !== hp[target] - damage && second !== null
            ? second
            : hp[target];
        const after = event.type === 'dodge' ? before : reported;
        if (event.type !== 'dodge') alt[target] = null;
        hp[target] = after;
        const attack: Pending | null = pending;
        pending = null;
        if (attack === null) {
          fail('adapter.resolution_without_attack', path);
          if (event.type !== 'dodge') alt[target] = before;
          continue;
        }
        if (attack.attacker !== event.attacker || attack.ability !== event.ability) {
          fail('adapter.attack_mismatch', path);
          continue;
        }
        if (damage > before || after !== before - damage) {
          fail('adapter.hp_mismatch', path, { expected: before - damage, actual: after });
          alt[target] = Math.max(0, before - damage);
          continue;
        }
        const result = event.type === 'hit' ? (event.crit ? 'crit' : 'hit') : event.type;
        cues.push({
          at_ms: event.t,
          type: 'strike',
          actor: event.attacker,
          move: attack.move,
          result,
          damage,
          target_hp: after,
        });
        continue;
      }
      case 'knockdown':
        cues.push({
          at_ms: event.t,
          type: 'knockdown',
          fighter: event.side,
          duration_ms: event.duration_ms,
        });
        continue;
      case 'ko':
        unresolved();
        kos.push({ side: event.side, t: event.t });
        cues.push({ at_ms: event.t, type: 'ko', fighter: event.side });
        continue;
      case 'low_hp':
      case 'moment':
        ignored[event.type]++;
        continue;
      case 'time_up':
        unresolved();
        timeUps.push({ t: event.t });
        cues.push({ at_ms: event.t, type: 'time_up' });
        continue;
      case 'end': {
        unresolved();
        end = { t: event.t, winner: event.winner };
        // Read as a string: a future variant of the protocol is not a type error here.
        const reason: string = event.reason;
        if (reason !== 'ko' && reason !== 'decision' && reason !== 'draw') {
          fail('adapter.unsupported_ending', path, { reason });
          continue;
        }
        const winner = event.winner;
        const ko = kos.length === 1 && timeUps.length === 0 ? kos[0] : undefined;
        const timeUp = timeUps.length === 1 && kos.length === 0 ? timeUps[0] : undefined;
        if (reason === 'ko' && ko !== undefined && winner !== null && winner !== ko.side) {
          cues.push({ at_ms: ko.t + WIN_DELAY_MS, type: 'win', fighter: winner });
        } else if (reason === 'decision' && timeUp !== undefined && winner !== null) {
          cues.push({ at_ms: timeUp.t + CLOSING_DELAY_MS, type: 'win', fighter: winner });
        } else if (reason === 'draw' && timeUp !== undefined && winner === null) {
          cues.push({ at_ms: timeUp.t + CLOSING_DELAY_MS, type: 'draw' });
        } else {
          fail('adapter.outcome_mismatch', path, { kos: kos.length, time_ups: timeUps.length });
        }
        continue;
      }
      default:
        // A future variant of the protocol: no cue, only counted.
        ignored.unknown++;
    }
  }

  if (liveMaxDurationMs === null) {
    unresolved();
    if (end === null) fail('adapter.end_missing', '/events');
  }
  if (errors.length > 0) return { ok: false, errors };
  const durationMs = end?.t ?? liveMaxDurationMs;
  if (durationMs === null) return { ok: false, errors };

  return {
    ok: true,
    timeline: {
      timeline_version: VERSION,
      duration_ms: durationMs,
      fighters: {
        a: { asset: fighters.a.asset, max_hp: header.a.max_hp },
        b: { asset: fighters.b.asset, max_hp: header.b.max_hp },
      },
      cues,
    },
    ignored,
  };
}
