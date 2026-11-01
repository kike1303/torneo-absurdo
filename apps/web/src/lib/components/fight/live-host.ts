import type { RendererHandle } from '@torneo/renderer';
import type { Timeline } from '@torneo/renderer/timeline';
import type { LiveSession } from '#lib/features/fight/live-session.js';

// How a live fight starts on a renderer (010 RF47, RF50, plan-c decision 11, plan-d decision 11). It
// lives here, with the handle and the session injected, so what happens when the renderer rejects
// can be tested without Pixi.
//
// The order matters: `loadLive` and, without waiting for it, `subscribe`, because `end` may come out
// while the sheets load and there would be no other publication. `live` is written synchronously in
// `loadLive`, so an immediate delivery is taken.

export interface LiveHostOptions {
  readonly handle: Pick<RendererHandle, 'loadLive' | 'extend' | 'play'>;
  readonly session: LiveSession;
  readonly timeline: Timeline;
  readonly assets: ReadonlyMap<string, unknown>;
  /** A monotonic clock, to measure each `extend`. */
  readonly now: () => number;
  readonly onExtended: (timeline: Timeline, ms: number) => void;
  /** Before `play`, with the sheets in. */
  readonly beforePlay?: () => void;
  /** Once at most: `extend` rejected, the adapter refused the events, or the load failed. */
  readonly onFail: (error: unknown) => void;
}

export interface LiveRun {
  /** `true` when it is playing; `false` if it failed or was cancelled before. */
  readonly done: Promise<boolean>;
  /** Stops the subscription, and nothing else from here on reaches the callbacks. */
  cancel(): void;
}

export function startLive(options: LiveHostOptions): LiveRun {
  const { handle, session } = options;
  let over = false;
  let unsubscribe: (() => void) | null = null;

  const cancel = (): void => {
    over = true;
    unsubscribe?.();
    unsubscribe = null;
  };
  const fail = (error: unknown): void => {
    if (over) return;
    cancel();
    options.onFail(error);
  };

  const loading = handle.loadLive(options.timeline, options.assets);
  const stop = session.subscribe(
    (next) => {
      if (over) return;
      try {
        const before = options.now();
        handle.extend(next);
        options.onExtended(next, options.now() - before);
      } catch (error) {
        fail(error);
      }
    },
    (code) => fail(Object.assign(new Error(code), { code })),
  );
  // A rejection in the immediate delivery of `subscribe` found nothing to cancel yet.
  if (over) stop();
  else unsubscribe = stop;

  const done = loading.then(
    () => {
      if (over) return false;
      options.beforePlay?.();
      handle.play();
      return true;
    },
    (error: unknown) => {
      fail(error);
      return false;
    },
  );
  return { done, cancel };
}
