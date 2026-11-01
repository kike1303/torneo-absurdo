import { describe, expect, it } from 'vitest';
import type { Timeline } from '@torneo/renderer/timeline';
import type { LiveSession } from '#lib/features/fight/live-session.js';
import { startLive } from './live-host.js';

// 010 plan-d decision 11: a rejection of `extend` and a `fail` of the adapter, in the immediate
// delivery of `subscribe` and in a later one: error state and subscription cancelled.
const TIMELINE = { duration_ms: 1 } as Timeline;
const NEXT = { duration_ms: 2 } as Timeline;
const REJECTED = Object.assign(new Error('timeline_invalid'), { code: 'timeline_invalid' });

type Push = (timeline: Timeline) => void;
type Fail = (code: string) => void;

function setup(
  options: {
    immediate?: (push: Push, fail: Fail) => void;
    rejectExtend?: boolean;
    load?: Promise<void>;
  } = {},
) {
  const calls: string[] = [];
  let push: Push = () => undefined;
  let failCode: Fail = () => undefined;
  let unsubscribed = 0;
  const session: LiveSession = {
    clock: () => 0,
    timeline: TIMELINE,
    subscribe(p, f) {
      calls.push('subscribe');
      push = p;
      failCode = f ?? (() => undefined);
      options.immediate?.(p, failCode);
      return () => {
        unsubscribed++;
      };
    },
  };
  const failures: unknown[] = [];
  const extended: Timeline[] = [];
  const run = startLive({
    handle: {
      loadLive: async () => {
        calls.push('loadLive');
        await (options.load ?? Promise.resolve());
      },
      extend: (timeline) => {
        calls.push('extend');
        if (options.rejectExtend === true) throw REJECTED;
        void timeline;
      },
      play: () => calls.push('play'),
    },
    session,
    timeline: TIMELINE,
    assets: new Map(),
    now: () => 0,
    onExtended: (timeline) => extended.push(timeline),
    beforePlay: () => calls.push('beforePlay'),
    onFail: (error) => failures.push(error),
  });
  return {
    run,
    calls,
    failures,
    extended,
    push: (t: Timeline) => push(t),
    fail: (c: string) => failCode(c),
    unsubscribed: () => unsubscribed,
  };
}

describe('startLive', () => {
  it('loads, subscribes without waiting, and plays with the sheets in', async () => {
    let release: () => void = () => undefined;
    const load = new Promise<void>((resolve) => (release = resolve));
    const { run, calls } = setup({ load });
    expect(calls).toEqual(['loadLive', 'subscribe']);
    release();
    expect(await run.done).toBe(true);
    expect(calls).toEqual(['loadLive', 'subscribe', 'beforePlay', 'play']);
  });

  it('an end delivered while the sheets load is taken', async () => {
    const { run, extended } = setup({ immediate: (push) => push(NEXT) });
    expect(extended).toEqual([NEXT]);
    expect(await run.done).toBe(true);
  });

  it('extends with each later timeline', async () => {
    const { run, push, extended } = setup();
    await run.done;
    push(NEXT);
    expect(extended).toEqual([NEXT]);
  });

  describe('extend rejected', () => {
    it('in the immediate delivery: error, subscription cancelled, no play', async () => {
      const { run, failures, unsubscribed, calls } = setup({
        immediate: (push) => push(NEXT),
        rejectExtend: true,
      });
      expect(failures).toEqual([REJECTED]);
      expect(unsubscribed()).toBe(1);
      expect(await run.done).toBe(false);
      expect(calls).not.toContain('play');
    });

    it('in a later delivery: error and subscription cancelled', async () => {
      const { run, push, failures, unsubscribed } = setup({ rejectExtend: true });
      await run.done;
      expect(unsubscribed()).toBe(0);
      push(NEXT);
      expect(failures).toEqual([REJECTED]);
      expect(unsubscribed()).toBe(1);
    });

    it('reports once, and later deliveries do nothing', async () => {
      const { run, push, failures, calls } = setup({ rejectExtend: true });
      await run.done;
      push(NEXT);
      push(NEXT);
      expect(failures).toHaveLength(1);
      expect(calls.filter((c) => c === 'extend')).toHaveLength(1);
    });
  });

  describe('fail of the adapter', () => {
    it('in the immediate delivery: error with its code, and cancelled', async () => {
      const { run, failures, unsubscribed } = setup({
        immediate: (_push, fail) => fail('timeline_invalid'),
      });
      expect(failures[0]).toMatchObject({ code: 'timeline_invalid' });
      expect(unsubscribed()).toBe(1);
      expect(await run.done).toBe(false);
    });

    it('in a later delivery: error with its code, and cancelled', async () => {
      const { run, fail, failures, unsubscribed } = setup();
      await run.done;
      fail('timeline_invalid');
      expect(failures[0]).toMatchObject({ code: 'timeline_invalid' });
      expect(unsubscribed()).toBe(1);
    });
  });

  it('a load that fails is an error, and it does not play', async () => {
    const error = Object.assign(new Error('x'), { code: 'asset_load_failed' });
    const { run, failures, calls, unsubscribed } = setup({ load: Promise.reject(error) });
    expect(await run.done).toBe(false);
    expect(failures).toEqual([error]);
    expect(unsubscribed()).toBe(1);
    expect(calls).not.toContain('play');
  });

  it('cancelled: no play, nothing reaches the callbacks', async () => {
    let release: () => void = () => undefined;
    const load = new Promise<void>((resolve) => (release = resolve));
    const { run, calls, push, extended, unsubscribed } = setup({ load });
    run.cancel();
    expect(unsubscribed()).toBe(1);
    push(NEXT);
    release();
    expect(await run.done).toBe(false);
    expect(calls).not.toContain('play');
    expect(extended).toEqual([]);
  });
});
