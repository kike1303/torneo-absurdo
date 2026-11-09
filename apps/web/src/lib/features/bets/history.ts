import type { BetHistoryItem, RefundReason, Side } from '@torneo/protocol';

// One row of the bet history (019): what the page draws of a fight the user bet on. Every amount
// comes from the server as it is; `lostAmount` of `presentation.ts` turns a net into what was lost.

export type HistoryStatus = 'pending' | 'won' | 'lost' | 'refunded';

export interface HistoryRow {
  readonly id: number;
  readonly a: string;
  readonly b: string;
  readonly side: Side;
  readonly staked: number;
  readonly bets: number;
  /** Milliseconds since the epoch, as the API gives them. */
  readonly placedAt: string;
  readonly status: HistoryStatus;
  /** What came back: the payout, or the refund. 0 while pending or lost. */
  readonly payout: number;
  /** The signed net of the fight: positive won, negative lost, 0 otherwise. */
  readonly net: number;
  /** Only on a refund: null if this build does not know the reason. */
  readonly refundReason: RefundReason | null;
}

const OUTCOMES: readonly HistoryStatus[] = ['won', 'lost', 'refunded'];

/**
 * A settled fight without a result, or with an outcome this build does not know, stays pending:
 * the page never makes up a result.
 */
export function historyRow(item: BetHistoryItem): HistoryRow {
  const result = item.settled ? item.result : null;
  const status: HistoryStatus =
    result !== null && OUTCOMES.includes(result.outcome) ? result.outcome : 'pending';
  const known = status === 'pending' ? null : result;
  return {
    id: item.fight_id,
    a: item.a,
    b: item.b,
    side: item.side,
    staked: item.staked,
    bets: item.bets,
    placedAt: item.placed_at,
    status,
    payout: known?.payout ?? 0,
    net: known?.net ?? 0,
    refundReason: status === 'refunded' ? (known?.refund_reason ?? null) : null,
  };
}
