// Live playback without a server (010 RF52): `?live=1` on /dev/arena releases an engine log by its
// `t`, in batches, the way the server does (RF40). Only for measuring and trying by hand.
import type { LiveSession } from '#lib/features/fight/live-session.js';
import {
  liveTimelineFromEvents,
  type AdaptedTimeline,
  type AdapterEvent,
  type AdapterLog,
} from '#lib/features/fight/timeline-from-fight-log.js';

type FighterSide = keyof AdaptedTimeline['fighters'];

/** The tick of RF40. */
export const DRIP_TICK_MS = 50;
/** `playback_delay_ms` of the spec; in phase D it comes from `FightSummary`. */
export const PLAYBACK_DELAY_MS = 3500;
/** `max_duration_ms` of the ruleset; in phase D it comes from `FightSummary`. */
export const MAX_DURATION_MS = 90_000;

export interface LogDripOptions {
  /** Server time already elapsed when opening (`?t=`): how to come in late. */
  readonly startElapsedMs: number;
  /** Any monotonic time in ms. */
  readonly now: () => number;
}

/** How many events are out at `elapsedMs`: those with `t ≤ elapsed`, so a same `t` goes together. */
export function releasedCount(events: readonly AdapterEvent[], elapsedMs: number): number {
  let count = 0;
  while (count < events.length && (events[count] as AdapterEvent).t <= elapsedMs) count++;
  return count;
}

export function openLogDrip(
  log: AdapterLog,
  assets: Readonly<Record<FighterSide, string>>,
  options: LogDripOptions,
): LiveSession {
  const events = log.events;
  const fighters = { a: { asset: assets.a }, b: { asset: assets.b } };
  const startedAt = options.now() - options.startElapsedMs;
  const elapsed = (): number => options.now() - startedAt;
  const timelineOf = (count: number): AdaptedTimeline => {
    const result = liveTimelineFromEvents(log, events.slice(0, count), fighters, MAX_DURATION_MS);
    if (!result.ok)
      throw Object.assign(new Error('timeline_invalid'), { code: 'timeline_invalid' as const });
    return result.timeline;
  };
  const opened = releasedCount(events, elapsed());

  return {
    clock: () => elapsed() - PLAYBACK_DELAY_MS,
    timeline: timelineOf(opened),
    subscribe(push) {
      let delivered = opened;
      let timer: ReturnType<typeof setInterval> | undefined;
      const stop = (): void => {
        clearInterval(timer);
        timer = undefined;
      };
      const tick = (): void => {
        const count = releasedCount(events, elapsed());
        if (count > delivered) {
          delivered = count;
          push(timelineOf(count));
        }
        if (delivered >= events.length) stop();
      };
      tick();
      if (delivered < events.length) timer = setInterval(tick, DRIP_TICK_MS);
      return stop;
    },
  };
}

export interface DurationStats {
  readonly count: number;
  readonly p50: number;
  readonly p95: number;
  readonly max: number;
}

/** Nearest-rank percentiles of the time each `extend` took, for the stats JSON. */
export function durationStats(samples: readonly number[]): DurationStats {
  const sorted = [...samples].sort((p, q) => p - q);
  const at = (q: number): number => {
    const rank = Math.max(1, Math.ceil(q * sorted.length));
    return sorted[rank - 1] ?? 0;
  };
  return { count: sorted.length, p50: at(0.5), p95: at(0.95), max: sorted.at(-1) ?? 0 };
}
