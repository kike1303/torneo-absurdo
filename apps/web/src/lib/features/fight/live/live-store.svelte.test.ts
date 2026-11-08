import { afterEach, describe, expect, it, vi } from 'vitest';
import type { FightPool, FightsCurrent, ServerMessage } from '@torneo/protocol';
import { FightStore } from './live-store.svelte.js';

// 012 RF35 and plan-c C3: the store keeps the newest `pool_update` for the panel, and the view does
// not change for it.
const BOARD: FightsCurrent = {
  server_time: '1000',
  halted: null,
  current: null,
  next: null,
  last: null,
};

afterEach(() => vi.unstubAllGlobals());

const poolAt = (time: string, stake: number): FightPool => ({
  fight_id: 7,
  a: { stake, bets: 1, payout_bp: null },
  b: { stake: 0, bets: 0, payout_bp: null },
  rake_bp: 0,
  server_time: time,
  settlement: null,
});

describe('a pool_update', () => {
  it('keeps the newest pool of each fight, never an older one', () => {
    const store = new FightStore(BOARD);
    store.notePool(poolAt('2000', 25));
    store.notePool(poolAt('1500', 10));
    expect(store.pools[7]?.a.stake).toBe(25);
    store.notePool(poolAt('2500', 50));
    expect(store.pools[7]?.a.stake).toBe(50);
  });

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
    expect(store.pools[7]?.a.stake).toBe(25);
    expect(fetchFn).not.toHaveBeenCalled();
  });
});
