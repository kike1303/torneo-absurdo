import type { EndReason, FightEvent, Side } from '@torneo/protocol';

// A fight without the canvas (010 RF54, plan-d decision 9): HP and result from the events, as pure
// functions of `t`. The result only from `t ≥ end.t`, never for having received `end`.

export interface EndOutcome {
  readonly winner: Side | null;
  readonly reason: EndReason;
}

export interface TextScene {
  readonly hp: Readonly<Record<Side, number>>;
  readonly ended: EndOutcome | null;
}

export type EndEvent = Extract<FightEvent, { type: 'end' }>;

export function endEvent(events: readonly FightEvent[]): EndEvent | null {
  const last = events.at(-1);
  return last?.type === 'end' ? last : null;
}

const other = (side: Side): Side => (side === 'a' ? 'b' : 'a');

export function textSceneAt(
  maxHp: Readonly<Record<Side, number>>,
  events: readonly FightEvent[],
  t: number,
): TextScene {
  const hp: Record<Side, number> = { a: maxHp.a, b: maxHp.b };
  let ended: EndOutcome | null = null;
  for (const event of events) {
    if (event.t > t) break;
    if (event.type === 'hit' || event.type === 'block') hp[other(event.attacker)] = event.hp;
    if (event.type === 'end') ended = { winner: event.winner, reason: event.reason };
  }
  return { hp, ended };
}

/** Whether something is still to come on screen at `t` (what is received, not what will be). */
export function hasPendingEvents(events: readonly FightEvent[], t: number): boolean {
  return (events.at(-1)?.t ?? Number.NEGATIVE_INFINITY) > t;
}
