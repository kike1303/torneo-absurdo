import { afterEach, describe, expect, it, vi } from 'vitest';
import type { FightsCurrent, ServerMessage } from '@torneo/protocol';
import { FightStore } from './live-store.svelte.js';

// 012 RF35 and plan-b step 1: the store ignores `pool_update` until phase C shows the panel.
const BOARD: FightsCurrent = {
  server_time: '1000',
  halted: null,
  current: null,
  next: null,
  last: null,
};

afterEach(() => vi.unstubAllGlobals());

describe('a pool_update', () => {
  it('does not change the view and reads nothing', () => {
    const fetchFn = vi.fn<typeof fetch>();
    vi.stubGlobal('fetch', fetchFn);
    const store = new FightStore(BOARD);
    const before = store.view;
    const deliver = (store as unknown as { onMessage(message: ServerMessage): void }).onMessage;
    deliver.call(store, {
      type: 'pool_update',
      pool: {
        fight_id: 7,
        a: { stake: 25, bets: 1, payout_bp: null },
        b: { stake: 0, bets: 0, payout_bp: null },
        rake_bp: 0,
        server_time: '2000',
        settlement: null,
      },
    });
    expect(store.view).toEqual(before);
    expect(store.board).toBe(BOARD);
    expect(store.serverNow).toBe(1000);
    expect(fetchFn).not.toHaveBeenCalled();
  });
});
