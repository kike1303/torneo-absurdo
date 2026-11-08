import type { FightDetail } from '@torneo/protocol';
import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import { ReplayStore } from '../src/lib/features/replay/replay-store.svelte.js';
import { realDeps } from './support/replay-engine.js';

// 011 RF15, RF18, RF20, RF21, RF22, RF24: the decisions of the page, with the real `.wasm`.
const VECTORS = JSON.parse(
  readFileSync(
    new URL('../../server/tests/golden/fight-verify-vectors.json', import.meta.url),
    'utf8',
  ),
) as readonly { name: string; detail: unknown }[];

function vector(name: string): FightDetail {
  const row = VECTORS.find((entry) => entry.name === name);
  if (row === undefined) throw new Error(name);
  return structuredClone(row.detail) as FightDetail;
}

const ID = 12;
const ok = (body: unknown) => async () => new Response(JSON.stringify(body));

async function opened(fetcher: typeof fetch, deps = realDeps()) {
  const store = new ReplayStore(ID, fetcher, deps);
  await store.start();
  return store;
}

describe('ReplayStore', () => {
  it('a real fight: ready to play, paused, the result hidden, and it matches', async () => {
    const fetcher = vi.fn<typeof fetch>(ok(vector('ko')));
    const store = await opened(fetcher);
    expect(fetcher).toHaveBeenCalledTimes(1);
    expect(store.load).toBe('ready');
    expect(store.stageVisible).toBe(true);
    expect(store.timeline).not.toBeNull();
    expect(store.resultSeen).toBe(false);
    expect(store.result?.kind).toBe('ko');
    expect(store.verdict).toEqual({ status: 'match', scope: 'fight' });
    expect(store.roundClock).toBe(86_000);
  });

  it('the result shows at the ending, or when finished, or when asked for, and it stays', async () => {
    for (const reach of [
      (s: ReplayStore) => s.onSnapshot({ t: 9000, phase: 'pose', ending: 'ko' }),
      (s: ReplayStore) => s.onSnapshot({ t: 9000, phase: 'finished', ending: null }),
      (s: ReplayStore) => s.showResult(),
    ]) {
      const store = await opened(ok(vector('ko')));
      store.onSnapshot({ t: 1000, phase: 'fight', ending: null });
      expect(store.resultSeen).toBe(false);
      reach(store);
      expect(store.resultSeen).toBe(true);
      store.onSnapshot({ t: 0, phase: 'intro', ending: null });
      expect(store.resultSeen).toBe(true);
    }
  });

  it('the round clock follows the snapshot, and stops at the ko', async () => {
    const store = await opened(ok(vector('ko')));
    store.onSnapshot({ t: 5000, phase: 'fight', ending: null });
    expect(store.roundClock).toBe(81_000);
    store.onSnapshot({ t: 89_000, phase: 'finished', ending: 'ko' });
    expect(store.roundClock).toBeGreaterThan(0);
  });

  it('webgl_unavailable: no stage, the notice, the button and the whole verdict', async () => {
    const store = await opened(ok(vector('ko')));
    store.onStageFailure('webgl_unavailable');
    expect(store.stageVisible).toBe(false);
    expect(store.noAnimation).toBe(true);
    expect(store.playbackError).toBe(false);
    expect(store.resultSeen).toBe(false);
    expect(store.verdict).toEqual({ status: 'match', scope: 'fight' });
  });

  it('another failure of the stage: the notice of playback', async () => {
    const store = await opened(ok(vector('ko')));
    store.onStageFailure('asset_load_failed');
    expect(store.stageVisible).toBe(false);
    expect(store.playbackError).toBe(true);
  });

  it.each(['log_null', 'log_not_json'])(
    'RF24, %s: no stage, the notice, the button, and a mismatch of the log',
    async (name) => {
      const store = await opened(ok(vector(name)));
      expect(store.load).toBe('ready');
      expect(store.stageVisible).toBe(false);
      expect(store.playbackError).toBe(true);
      expect(store.result?.kind).toBe('ko');
      expect(store.verdict).toEqual({ status: 'mismatch', check: 'log' });
    },
  );

  it('RF24, a spec missing: it still plays, and a mismatch of the snapshot', async () => {
    const store = await opened(ok(vector('spec_a_null')));
    expect(store.stageVisible).toBe(true);
    expect(store.verdict).toEqual({ status: 'mismatch', check: 'snapshot' });
  });

  it.each([
    ['without its shape', { fight: {} }, 'data'],
    ['not an object', [1, 2], 'data'],
  ])('a response %s: only the mismatch', async (_name, body, check) => {
    const store = await opened(ok(body));
    expect(store.load).toBe('untrusted');
    expect(store.detail).toBeNull();
    expect(store.stageVisible).toBe(false);
    expect(store.result).toBeNull();
    expect(store.verdict).toEqual({ status: 'mismatch', check });
  });

  // Security review of the amendment: with a seed and a log, an altered response shows nothing else.
  it('another id, with a seed and a log: only the mismatch', async () => {
    const detail = vector('ko');
    detail.fight.id = ID + 1;
    const store = await opened(ok(detail));
    expect(store.load).toBe('untrusted');
    expect(store.detail).toBeNull();
    expect(store.timeline).toBeNull();
    expect(store.stageVisible).toBe(false);
    expect(store.result).toBeNull();
    expect(store.verdict).toEqual({ status: 'mismatch', check: 'data' });
  });

  it.each(['state_live', 'state_unknown', 'final_null'])(
    '%s, with a seed and a log: only the mismatch',
    async (name) => {
      const store = await opened(ok(vector(name)));
      expect(store.load).toBe('untrusted');
      expect(store.detail).toBeNull();
      expect(store.timeline).toBeNull();
      expect(store.stageVisible).toBe(false);
      expect(store.result).toBeNull();
      expect(store.verdict).toEqual({ status: 'mismatch', check: 'state' });
    },
  );

  it('a read that fails has a retry, and no verdict at all', async () => {
    const fetcher = vi.fn<typeof fetch>(async () => new Response('{}', { status: 503 }));
    const store = await opened(fetcher);
    expect(store.load).toBe('unavailable');
    expect(store.verdict).toBeNull();
    fetcher.mockImplementation(ok(vector('ko')));
    await store.start();
    expect(store.load).toBe('ready');
    expect(store.verdict).toEqual({ status: 'match', scope: 'fight' });
  });

  it('a fight without a log has no stage nor playback notice, and it is verified', async () => {
    const store = await opened(ok(vector('cancelled_missed_window')));
    expect(store.load).toBe('ready');
    expect(store.decided).toBe(false);
    expect(store.stageVisible).toBe(false);
    expect(store.playbackError).toBe(false);
    expect(store.verdict).toEqual({ status: 'match', scope: 'snapshot' });
  });

  it('RF22: an unknown reason does not break, with or without a seed', async () => {
    const store = await opened(ok(vector('unknown_voided_reason_without_a_seed')));
    expect(store.load).toBe('ready');
    expect(store.verdict).toEqual({ status: 'cannot_check', reason: 'unknown_final' });
  });

  it('an unknown kind or decided reason has no result to show', async () => {
    for (const name of ['unknown_final_kind', 'unknown_decided_reason']) {
      const store = await opened(ok(vector(name)));
      expect(store.result, name).toBeNull();
      expect(store.verdict, name).toEqual({ status: 'cannot_check', reason: 'unknown_final' });
    }
  });

  it('with the engine down, the fight still plays, and a retry can pass', async () => {
    const down = realDeps({ engine: () => Promise.reject(new Error('blocked')) });
    const engine = vi.fn(down.engine);
    const store = await opened(ok(vector('ko')), { ...down, engine });
    expect(store.stageVisible).toBe(true);
    expect(store.verdict).toEqual({ status: 'cannot_check', reason: 'engine_unavailable' });
    engine.mockImplementation(realDeps().engine);
    store.retryVerification();
    await vi.waitFor(() => expect(store.verdict).toEqual({ status: 'match', scope: 'fight' }));
  });
});
