import { describe, expect, it } from 'vitest';
import type { FightPool } from '@torneo/protocol';
import { lostAmount, multiplier, poolShares, rakeShare } from './presentation.js';

// 012 RF43: what the bar and the multiplier show.
const pool = (a: number, b: number): FightPool => ({
  fight_id: 1,
  a: { stake: a, bets: 1, payout_bp: null },
  b: { stake: b, bets: 1, payout_bp: null },
  rake_bp: 0,
  server_time: '0',
  settlement: null,
});
describe('presentation', () => {
  it('shows payout_bp as a multiplier, and nothing while a side is empty', () => {
    expect(multiplier(19_000)).toBe(1.9);
    expect(multiplier(10_000)).toBe(1);
    expect(multiplier(null)).toBeNull();
  });

  it('says what a lost bet cost', () => {
    expect(lostAmount(-40)).toBe(40);
  });

  it('gives the rake as a share', () => {
    expect(rakeShare(500)).toBe(0.05);
    expect(rakeShare(0)).toBe(0);
  });

  it('splits the bar by the stakes as they are', () => {
    expect(poolShares(pool(30, 10))).toEqual({ a: 0.75, b: 0.25 });
    expect(poolShares(pool(0, 10))).toEqual({ a: 0, b: 1 });
    expect(poolShares(pool(0, 0))).toEqual({ a: 0, b: 0 });
  });
});
