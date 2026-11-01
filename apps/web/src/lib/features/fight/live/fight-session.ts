import type { FightEvent, FightSummary } from '@torneo/protocol';
import type { LiveSession } from '../live-session.js';
import { liveTimelineFromEvents, type AdaptedTimeline } from '../timeline-from-fight-log.js';
import { endEvent } from './text-scene.js';

// A `LiveSession` over the events of a fight (010 plan-d decision 7). It resolves when the `intro`
// is in and it is up to date: it has `end`, or a while went by without another page. It does not
// depend on the size of the pages. So `loadLive` waits for the `intro`, an empty list never reaches
// the adapter, and it is known whether `end` already went by.

/** Quiet time that means there is nothing else behind. */
export const UP_TO_DATE_MS = 300;

export interface FightSessionDeps {
  readonly summary: () => FightSummary;
  readonly events: () => readonly FightEvent[];
  readonly serverNow: () => number;
  readonly assets: { readonly a: string; readonly b: string };
  readonly setTimer: (run: () => void, ms: number) => unknown;
  readonly clearTimer: (timer: unknown) => void;
  /** Up to date with `end.t` already gone by: there is nothing to play. */
  readonly onLate: () => void;
}

export interface FightSession {
  /** Pending until it is up to date; it never resolves if there is nothing to play. */
  readonly ready: Promise<LiveSession>;
  /** A page was placed, or the summary changed. */
  changed(): void;
  stop(): void;
}

const invalid = () => Object.assign(new Error('timeline_invalid'), { code: 'timeline_invalid' });

export function createFightSession(deps: FightSessionDeps): FightSession {
  let quiet: unknown = null;
  let settled = false;
  let stopped = false;
  let scheduled = false;
  let resolve!: (session: LiveSession) => void;
  let reject!: (reason: unknown) => void;
  const ready = new Promise<LiveSession>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  // A rejection reaches whoever awaits; without anyone yet, it must not be an unhandled one.
  ready.catch(() => undefined);

  const subscribers = new Set<{
    push: (timeline: AdaptedTimeline) => void;
    fail: (code: string) => void;
    delivered: number;
  }>();

  const clockOf = (): number => {
    const fight = deps.summary();
    return deps.serverNow() - Number(fight.live_started_at) - fight.playback_delay_ms;
  };

  function timelineOf(count: number): AdaptedTimeline | null {
    const fight = deps.summary();
    const result = liveTimelineFromEvents(
      fight,
      deps.events().slice(0, count),
      { a: { asset: deps.assets.a }, b: { asset: deps.assets.b } },
      fight.max_duration_ms,
    );
    return result.ok ? result.timeline : null;
  }

  function flush(): void {
    scheduled = false;
    const count = deps.events().length;
    for (const subscriber of [...subscribers]) {
      if (subscriber.delivered >= count) continue;
      subscriber.delivered = count;
      const timeline = timelineOf(count);
      if (timeline === null) subscriber.fail('timeline_invalid');
      else subscriber.push(timeline);
    }
  }

  function settle(): void {
    quiet = null;
    if (settled || stopped) return;
    settled = true;
    const events = deps.events();
    const end = endEvent(events);
    if (end !== null && clockOf() >= end.t) {
      deps.onLate();
      return;
    }
    const opened = events.length;
    const timeline = timelineOf(opened);
    if (timeline === null) {
      reject(invalid());
      return;
    }
    resolve({
      clock: clockOf,
      timeline,
      subscribe(push, fail = () => undefined) {
        const subscriber = { push, fail, delivered: opened };
        subscribers.add(subscriber);
        // An `end` released while the sheets load is not lost.
        if (deps.events().length > opened) flush();
        return () => subscribers.delete(subscriber);
      },
    });
  }

  return {
    ready,
    changed() {
      if (stopped) return;
      if (settled) {
        if (scheduled) return;
        scheduled = true;
        // The pages of one tick go together in one `push`.
        queueMicrotask(() => {
          if (!stopped) flush();
        });
        return;
      }
      const events = deps.events();
      if (events[0]?.type !== 'intro' || deps.summary().live_started_at === null) return;
      if (quiet !== null) deps.clearTimer(quiet);
      quiet = null;
      if (endEvent(events) !== null) settle();
      else quiet = deps.setTimer(settle, UP_TO_DATE_MS);
    },
    stop() {
      stopped = true;
      if (quiet !== null) deps.clearTimer(quiet);
      quiet = null;
      subscribers.clear();
    },
  };
}
