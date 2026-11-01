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

/** `m:ss`, and `h:mm:ss` from one hour on. Rounds up: the zero only shows at the zero. */
export function formatClock(ms: number, locale: string): string {
  const total = Math.ceil(Math.max(0, ms) / 1000);
  const digits = (n: number, min: number) =>
    new Intl.NumberFormat(locale, { minimumIntegerDigits: min, useGrouping: false }).format(n);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return hours > 0
    ? `${digits(hours, 1)}:${digits(minutes, 2)}:${digits(seconds, 2)}`
    : `${digits(minutes, 1)}:${digits(seconds, 2)}`;
}
