import { describe, expect, it } from 'vitest';
import type { BailoutState, ErrorCode, MyFightBets } from '@torneo/protocol';
import { board, mine, pool, summary, wallet } from './fixtures.js';
import { panelState, rejectionOf, type PanelInput } from './panel-state.js';

// 012 RF42: a row per state and per code.
const OPEN = summary(2);
const LAST = summary(1, {
  state: 'settled',
  final: { kind: 'decided', winner: 'a', reason: 'ko' },
});

function input(over: Partial<PanelInput> = {}): PanelInput {
  return {
    signedIn: true,
    board: board({ current: OPEN, last: LAST }),
    wallet: wallet(),
    mine: new Map([[2, mine(2)]]),
    pool: pool(2, 100, 50),
    shownResult: null,
    failure: null,
    ...over,
  };
}

const bets = (...entries: MyFightBets[]) => new Map(entries.map((m) => [m.fight_id, m]));

describe('the fight zone', () => {
  it('without a session: the pool and no form', () => {
    const state = panelState(input({ signedIn: false, wallet: null }));
    expect(state.fight).toEqual({ kind: 'signed_out', fight: OPEN, pool: pool(2, 100, 50) });
    expect(state.bonus).toBeNull();
  });

  it('loading or a failed read: the pool if there is one, never an invented balance', () => {
    expect(panelState(input({ wallet: null })).fight).toEqual({
      kind: 'unavailable',
      fight: OPEN,
      pool: pool(2, 100, 50),
    });
    expect(panelState(input({ mine: new Map() })).fight.kind).toBe('unavailable');
  });

  it('open: the form, with both sides', () => {
    const fight = panelState(input()).fight;
    expect(fight.kind).toBe('open');
    if (fight.kind === 'open') {
      expect(fight.oneSided).toBe(false);
      expect(fight.rejection).toBeNull();
    }
  });

  it('a side empty, or no pool yet: the one-sided notice', () => {
    for (const p of [pool(2, 100, 0), null, pool(9, 1, 1)]) {
      const fight = panelState(input({ pool: p })).fight;
      expect(fight.kind === 'open' && fight.oneSided).toBe(true);
    }
  });

  it('placed: still the form, to add on the same side', () => {
    const placed = mine(2, { side: 'a', staked: 100, max_stake: 300 });
    const fight = panelState(input({ mine: bets(placed) })).fight;
    expect(fight.kind === 'open' && fight.mine).toEqual(placed);
  });

  it.each<BailoutState>(['available', 'grant_first', 'not_eligible', 'already_claimed'])(
    'without coins (%s): before "cannot add more", with its bet',
    (bailout) => {
      const placed = mine(2, { side: 'a', staked: 25, max_stake: 0 });
      const state = panelState(
        input({ wallet: wallet({ balance: 24, bailout }), mine: bets(placed) }),
      );
      expect(state.fight).toEqual({
        kind: 'broke',
        bailout,
        wallet: wallet({ balance: 24, bailout }),
        placed: { fight: OPEN, mine: placed },
      });
    },
  );

  it('without coins and without a target fight: still the way out', () => {
    const state = panelState(
      input({ board: board({ last: LAST }), wallet: wallet({ balance: 0, bailout: 'available' }) }),
    );
    expect(state.fight.kind).toBe('broke');
  });

  it('balance at the minimum and nothing more to add: "cannot add more"', () => {
    const capped = mine(2, { side: 'b', staked: 500, max_stake: 0 });
    const state = panelState(input({ wallet: wallet({ balance: 25 }), mine: bets(capped) }));
    expect(state.fight).toEqual({ kind: 'capped', fight: OPEN, mine: capped });
  });

  it('a full pool without own bets is "cannot add more" too', () => {
    const state = panelState(input({ mine: bets(mine(2, { max_stake: 0 })) }));
    expect(state.fight.kind).toBe('capped');
  });

  it('halted: no form', () => {
    const state = panelState(input({ board: board({ current: OPEN, halted: true }) }));
    expect(state.fight).toEqual({ kind: 'none' });
  });
});

describe('the bet on the current fight', () => {
  const placed = mine(2, { side: 'a', staked: 50 });

  it.each(['betting_closed', 'live'] as const)('in %s, with the form of the next one', (s) => {
    const current = summary(2, { state: s });
    const next = summary(3);
    const state = panelState(
      input({ board: board({ current, next }), mine: bets(placed, mine(3)) }),
    );
    expect(state.placed).toEqual({ fight: current, mine: placed, halted: false });
    expect(state.fight.kind).toBe('open');
  });

  it('halted: its bet, kept', () => {
    const current = summary(2, { state: 'live' });
    const state = panelState(
      input({ board: board({ current, halted: true }), mine: bets(placed) }),
    );
    expect(state.placed?.halted).toBe(true);
    expect(state.fight).toEqual({ kind: 'none' });
  });

  it('without a bet, or open, or without a session: nothing', () => {
    const current = summary(2, { state: 'live' });
    expect(panelState(input({ board: board({ current }) })).placed).toBeNull();
    expect(panelState(input({ mine: bets(placed) })).placed).toBeNull();
    expect(
      panelState(input({ signedIn: false, board: board({ current }), mine: bets(placed) })).placed,
    ).toBeNull();
  });
});

describe('the result of last (RF45)', () => {
  const won = mine(1, {
    side: 'a',
    staked: 50,
    max_stake: 0,
    result: { outcome: 'won', payout: 90, net: 40, refund_reason: null },
  });

  it('only while the view shows that fight', () => {
    expect(panelState(input({ mine: bets(won, mine(2)) })).result).toBeNull();
    expect(panelState(input({ mine: bets(won, mine(2)), shownResult: 1 })).result).toEqual({
      fight: LAST,
      result: won.result,
    });
    expect(panelState(input({ mine: bets(won, mine(2)), shownResult: 7 })).result).toBeNull();
  });

  it('together with the form of the next fight', () => {
    const state = panelState(input({ mine: bets(won, mine(2)), shownResult: 1 }));
    expect(state.result).not.toBeNull();
    expect(state.fight.kind).toBe('open');
  });

  it('without a bet on it: none', () => {
    expect(panelState(input({ mine: bets(mine(1), mine(2)), shownResult: 1 })).result).toBeNull();
  });
});

describe('the bonus', () => {
  it('whenever it is available: with a balance, and without a target fight', () => {
    const available = wallet({ bonus_available: true });
    expect(panelState(input({ wallet: available })).bonus).toBe(500);
    expect(panelState(input({ wallet: available, board: board() })).bonus).toBe(500);
    expect(panelState(input()).bonus).toBeNull();
  });
});

describe('a refusal', () => {
  const room = mine(2, { side: 'a', staked: 100, max_stake: 300 });
  const full = mine(2, { side: 'a', staked: 500, max_stake: 0 });

  it.each<[ErrorCode | null, MyFightBets, string]>([
    ['bet.window_closed', room, 'state'],
    ['bet.below_minimum', room, 'minimum'],
    ['bet.side_locked', room, 'side'],
    ['bet.fight_cap_exceeded', room, 'maximum'],
    ['bet.fight_cap_exceeded', full, 'capped'],
    ['bet.insufficient_balance', room, 'maximum'],
    ['bet.too_many_bets', room, 'capped'],
    ['bet.pool_full', room, 'capped'],
    ['bet.key_reused', room, 'reload'],
    ['bet.invalid_request', room, 'reload'],
    ['bet.unavailable', room, 'retry'],
    ['http.rate_limited', room, 'retry'],
    ['auth.try_again', room, 'retry'],
    [null, room, 'retry'],
  ])('%s gives its exit', (code, m, exit) => {
    expect(rejectionOf(code, m).exit).toBe(exit);
  });

  it('shows on the open form, with what was typed kept by the caller', () => {
    const failure = {
      code: 'bet.side_locked' as ErrorCode,
      amount: '40',
      side: 'b' as const,
      clientKey: null,
    };
    const fight = panelState(input({ mine: bets(room), failure })).fight;
    expect(fight.kind === 'open' && fight.rejection).toEqual({
      code: 'bet.side_locked',
      exit: 'side',
    });
  });
});
