import type { FightEvent } from '@torneo/protocol';

// The clock of the round (010 RF55, plan-d decision 12): how much of `time_limit_ms` is left at `t`.
// It stops at the first `ko` or `time_up` whose moment already came: decided by `t`, never by having
// received the event, so it gives nothing away early.

export function roundClockMs(
  timeLimitMs: number,
  events: readonly FightEvent[],
  t: number,
): number {
  const stop = events.find((e) => (e.type === 'ko' || e.type === 'time_up') && e.t <= t);
  const shown = stop?.t ?? t;
  return Math.min(timeLimitMs, Math.max(0, timeLimitMs - Math.max(0, shown)));
}
