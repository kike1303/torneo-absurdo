// The narrow adapter from the engine log to the renderer timeline v1 (002 RF-C1, plan-c decisions
// 1 to 3). The only file that knows both shapes (tests/fight-boundary.test.ts). Pure: no I/O, no
// clock and no randomness; codes only, no text (ADR-0007). It never fixes nor invents anything
// (RF-C4): what v1 can check is left to `validateTimeline` in `load`.
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

/** Gap between `ko` and `win` (equal to `TIMELINE_LIMITS.winGapMs`, checked by a test). */
export const WIN_DELAY_MS = 1500;
/** Equal to `TIMELINE_VERSION` (checked by a test): only values may not be imported here. */
const VERSION = 1;
const MAX_ERRORS = 100;

const MOVE_BY_KIND: Readonly<Record<AbilityKind, Move>> = {
  strike: 'attack_light',
  heavy: 'attack_heavy',
  special: 'special',
};

const other = (side: Side): Side => (side === 'a' ? 'b' : 'a');

interface Pending {
  readonly attacker: Side;
  readonly ability: string;
  readonly move: Move;
  readonly index: number;
}

export function timelineFromFightLog(
  log: FightLog,
  fighters: Readonly<Record<Side, { readonly asset: string }>>,
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

  const events: readonly FightEvent[] = log.events;
  const first = events[0];
  if (first === undefined || first.type !== 'intro' || first.t !== 0)
    fail('adapter.intro_missing', '/events/0');

  const hp: Record<Side, number> = { a: log.a.max_hp, b: log.b.max_hp };
  const ignored = { low_hp: 0, moment: 0, unknown: 0 };
  const cues: Cue[] = [];
  const kos: { readonly side: Side; readonly t: number }[] = [];
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
    if (event.t < previousT) fail('adapter.unsorted', path);
    previousT = Math.max(previousT, event.t);

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
        // The HP chain goes on through any error, so one bad event gives one error.
        const target = other(event.attacker);
        const before = hp[target];
        const damage = event.type === 'dodge' ? 0 : event.damage;
        const after = event.type === 'dodge' ? before : event.hp;
        hp[target] = Math.max(0, before - damage);
        const attack: Pending | null = pending;
        pending = null;
        if (attack === null) {
          fail('adapter.resolution_without_attack', path);
          continue;
        }
        if (attack.attacker !== event.attacker || attack.ability !== event.ability) {
          fail('adapter.attack_mismatch', path);
          continue;
        }
        if (damage > before || after !== before - damage) {
          fail('adapter.hp_mismatch', path, { expected: before - damage, actual: after });
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
        fail('adapter.unsupported_ending', path);
        continue;
      case 'end':
        unresolved();
        end = { t: event.t, winner: event.winner };
        if (event.reason !== 'ko') {
          fail('adapter.unsupported_ending', path, { reason: event.reason });
          continue;
        }
        if (kos.length !== 1 || event.winner === null || event.winner === kos[0]?.side) {
          fail('adapter.outcome_mismatch', path, { kos: kos.length });
          continue;
        }
        cues.push({ at_ms: (kos[0]?.t ?? 0) + WIN_DELAY_MS, type: 'win', fighter: event.winner });
        continue;
      default:
        // A future variant of the protocol: no cue, only counted.
        ignored.unknown++;
    }
  }

  if (end === null) fail('adapter.end_missing', '/events');
  if (errors.length > 0 || end === null) return { ok: false, errors };

  return {
    ok: true,
    timeline: {
      timeline_version: VERSION,
      duration_ms: end.t,
      fighters: {
        a: { asset: fighters.a.asset, max_hp: log.a.max_hp },
        b: { asset: fighters.b.asset, max_hp: log.b.max_hp },
      },
      cues,
    },
    ignored,
  };
}
