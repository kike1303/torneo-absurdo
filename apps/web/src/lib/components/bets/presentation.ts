import type { FightPool, Side } from '@torneo/protocol';

// 012 RF43, plan-c C6: the only file of the panel that does arithmetic on coins, and only to draw.
// Every amount, cap, estimate and net comes from the server; here the pool becomes the share of a
// bar, and `payout_bp` the multiplier it already is.

/** `payout_bp` as a multiplier (`19_000` → 1.9); `null` while a side is empty. */
export function multiplier(payoutBp: number | null): number | null {
  return payoutBp === null ? null : payoutBp / 10_000;
}

/** The share of each side in the pool, from 0 to 1; both 0 with an empty pool. */
export function poolShares(pool: FightPool): Record<Side, number> {
  const total = pool.a.stake + pool.b.stake;
  if (total <= 0) return { a: 0, b: 0 };
  return { a: pool.a.stake / total, b: pool.b.stake / total };
}

/** The rake of a pool as a share from 0 to 1, to format as a percent. */
export function rakeShare(rakeBp: number): number {
  return rakeBp / 10_000;
}

/** What a lost bet cost, from its `net` (always negative or 0): to say "you lost 40". */
export function lostAmount(net: number): number {
  return -net;
}
