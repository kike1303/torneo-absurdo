import type { FightPool, FightSummary, FightsCurrent, MyFightBets, Wallet } from '@torneo/protocol';

// Builders for the tests of the panel: every value a test does not name is a plain default.

export function summary(id: number, over: Partial<FightSummary> = {}): FightSummary {
  return {
    id,
    state: 'betting_open',
    a: { slug: 'a', max_hp: 100 },
    b: { slug: 'b', max_hp: 100 },
    ruleset_version: 1,
    snapshot_sha256: 'x',
    seed_commit: 'c',
    opens_at: '0',
    closes_at: '1000',
    live_at: '2000',
    live_started_at: null,
    time_limit_ms: 86_000,
    max_duration_ms: 90_000,
    playback_delay_ms: 3500,
    final: null,
    ...over,
  };
}

export function board(over: Partial<FightsCurrent> = {}): FightsCurrent {
  return { server_time: '0', halted: false, current: null, next: null, last: null, ...over };
}

export function wallet(over: Partial<Wallet> = {}): Wallet {
  return {
    balance: 500,
    open_stakes: 0,
    bonus_available: false,
    bailout: 'not_eligible',
    bonus_amount: 500,
    bailout_amount: 250,
    min_bet: 25,
    fight_cap: 500,
    day_resets_at: '86400000',
    probation: null,
    ...over,
  };
}

export function mine(fightId: number, over: Partial<MyFightBets> = {}): MyFightBets {
  return {
    fight_id: fightId,
    side: null,
    staked: 0,
    max_stake: 500,
    bets: [],
    result: null,
    ...over,
  };
}

export function pool(fightId: number, a: number, b: number): FightPool {
  return {
    fight_id: fightId,
    a: { stake: a, bets: a > 0 ? 1 : 0, payout_bp: a > 0 && b > 0 ? 19_000 : null },
    b: { stake: b, bets: b > 0 ? 1 : 0, payout_bp: a > 0 && b > 0 ? 19_000 : null },
    rake_bp: 0,
    server_time: '0',
    settlement: null,
  };
}
