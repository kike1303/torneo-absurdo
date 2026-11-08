import type { BetHistoryItem, MyFightResult } from '@torneo/protocol';
import { describe, expect, it } from 'vitest';
import { historyRow } from './history.js';

function item(over: Partial<BetHistoryItem> = {}): BetHistoryItem {
  return {
    fight_id: 7,
    a: 'striker',
    b: 'tank',
    side: 'a',
    staked: 40,
    bets: 2,
    placed_at: '1000',
    settled: true,
    result: null,
    ...over,
  };
}

const result = (over: Partial<MyFightResult>): MyFightResult => ({
  outcome: 'won',
  payout: 80,
  net: 40,
  refund_reason: null,
  ...over,
});

describe('historyRow (019)', () => {
  it('keeps the fight, the side and the stakes as the server gives them', () => {
    const row = historyRow(item({ side: 'b', settled: false }));
    expect(row).toMatchObject({ id: 7, a: 'striker', b: 'tank', side: 'b', staked: 40, bets: 2 });
    expect(row.placedAt).toBe('1000');
    expect(row.status).toBe('pending');
    expect([row.payout, row.net, row.refundReason]).toEqual([0, 0, null]);
  });

  it('reads a won, a lost and a refunded fight', () => {
    expect(historyRow(item({ result: result({}) }))).toMatchObject({
      status: 'won',
      payout: 80,
      net: 40,
    });
    expect(
      historyRow(item({ result: result({ outcome: 'lost', payout: 0, net: -40 }) })),
    ).toMatchObject({ status: 'lost', payout: 0, net: -40, refundReason: null });
    expect(
      historyRow(
        item({
          result: result({ outcome: 'refunded', payout: 40, net: 0, refund_reason: 'draw' }),
        }),
      ),
    ).toMatchObject({ status: 'refunded', payout: 40, refundReason: 'draw' });
  });

  it('never makes up a result', () => {
    // Not settled: a result that came anyway is not shown.
    expect(historyRow(item({ settled: false, result: result({}) })).status).toBe('pending');
    // Settled without a result, or with an outcome this build does not know.
    expect(historyRow(item({ result: null })).status).toBe('pending');
    const unknown = result({ outcome: 'jackpot' as MyFightResult['outcome'] });
    expect(historyRow(item({ result: unknown }))).toMatchObject({ status: 'pending', payout: 0 });
    // A reason only on a refund.
    expect(historyRow(item({ result: result({ refund_reason: 'draw' }) })).refundReason).toBe(null);
  });
});
