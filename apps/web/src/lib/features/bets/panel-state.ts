import type {
  BailoutState,
  BetErrorCode,
  ErrorCode,
  FightPool,
  FightSummary,
  FightsCurrent,
  MyFightBets,
  MyFightResult,
  Side,
  Wallet,
} from '@torneo/protocol';
import { targetFight } from './target.js';

// 012 RF42, plan-c C7: what the betting panel shows, decided in one pure function. Three zones that
// may live together: the bonus, the result of `last`, and the fight. Comparing `balance` with
// `min_bet` picks a state; it computes no money (RF43).

/** The codes a bet can be refused with, as the panel tells them apart. */
export type RejectionCode = BetErrorCode | 'http.rate_limited';

/** Where a refusal sends the user (the "Salida" column of RF42). */
export type RejectionExit =
  'state' | 'minimum' | 'side' | 'maximum' | 'capped' | 'reload' | 'retry';

export interface Rejection {
  readonly code: RejectionCode;
  readonly exit: RejectionExit;
}

/** What the `?/bet` action gave back on a refusal: the code, and what was typed, to keep it. */
export interface BetFailure {
  readonly code: ErrorCode | null;
  readonly amount: string;
  readonly side: Side | null;
  /** The key that was posted (RF42: a retry is the same bet). */
  readonly clientKey: string | null;
}

export interface PanelInput {
  readonly signedIn: boolean;
  readonly board: FightsCurrent | null;
  /** `null` while it was not read, or its read failed. */
  readonly wallet: Wallet | null;
  /** The bets of the session by fight id, as read; a fight missing here was not read. */
  readonly mine: ReadonlyMap<number, MyFightBets>;
  /** The newest pool of the target fight, if any. */
  readonly pool: FightPool | null;
  /** The fight whose result the view shows now (010 RF54), or `null`. */
  readonly shownResult: number | null;
  readonly failure: BetFailure | null;
}

export interface OpenFight {
  readonly kind: 'open';
  readonly fight: FightSummary;
  readonly pool: FightPool | null;
  /** A side is empty: if it stays so at the close, everything comes back. */
  readonly oneSided: boolean;
  readonly wallet: Wallet;
  readonly mine: MyFightBets;
  readonly rejection: Rejection | null;
}

export type FightZone =
  | { readonly kind: 'none' }
  | {
      readonly kind: 'signed_out';
      readonly fight: FightSummary | null;
      readonly pool: FightPool | null;
    }
  | {
      readonly kind: 'unavailable';
      readonly fight: FightSummary | null;
      readonly pool: FightPool | null;
    }
  | OpenFight
  | {
      readonly kind: 'broke';
      readonly bailout: BailoutState;
      readonly wallet: Wallet;
      /** The target fight and its bet, if it has one. */
      readonly placed: { readonly fight: FightSummary; readonly mine: MyFightBets } | null;
    }
  | { readonly kind: 'capped'; readonly fight: FightSummary; readonly mine: MyFightBets };

/** The bet on the `current` once its window closed: in `betting_closed`, live or halted. */
export interface Placed {
  readonly fight: FightSummary;
  readonly mine: MyFightBets;
  readonly halted: boolean;
}

export interface ShownResult {
  readonly fight: FightSummary;
  readonly result: MyFightResult;
}

export interface PanelState {
  /** The daily bonus that can be claimed now, shown whenever it can, with a balance or not. */
  readonly bonus: number | null;
  readonly result: ShownResult | null;
  /** Its bet on the `current`, also after a reload in the middle of the fight (RF44). */
  readonly placed: Placed | null;
  readonly fight: FightZone;
}

const placedOn = (mine: MyFightBets | undefined): mine is MyFightBets =>
  mine !== undefined && mine.staked > 0;

/** The exit of each code (RF42), with the facts read again after it. */
export function rejectionOf(code: ErrorCode | null, mine: MyFightBets | null): Rejection {
  switch (code) {
    case 'bet.window_closed':
      return { code, exit: 'state' };
    case 'bet.below_minimum':
      return { code, exit: 'minimum' };
    case 'bet.side_locked':
      return { code, exit: 'side' };
    case 'bet.fight_cap_exceeded':
      return { code, exit: mine !== null && mine.max_stake > 0 ? 'maximum' : 'capped' };
    case 'bet.insufficient_balance':
      return { code, exit: 'maximum' };
    case 'bet.too_many_bets':
    case 'bet.pool_full':
      return { code, exit: 'capped' };
    case 'bet.key_reused':
    case 'bet.invalid_request':
      return { code, exit: 'reload' };
    case 'http.rate_limited':
      return { code, exit: 'retry' };
    default:
      // `bet.unavailable`, a code of another domain, or none: try again in a moment.
      return { code: 'bet.unavailable', exit: 'retry' };
  }
}

/** The result of `last` while the view shows it (RF45): never before, and never another one. */
function shownResultOf(input: PanelInput): ShownResult | null {
  const last = input.board?.last ?? null;
  if (last === null || input.shownResult !== last.id) return null;
  const result = input.mine.get(last.id)?.result ?? null;
  return result === null ? null : { fight: last, result };
}

function fightZone(input: PanelInput): FightZone {
  const { board, wallet, pool } = input;
  const target = targetFight(board);
  const targetPool = target !== null && pool?.fight_id === target.id ? pool : null;
  if (!input.signedIn) {
    return target === null
      ? { kind: 'none' }
      : { kind: 'signed_out', fight: target, pool: targetPool };
  }
  if (wallet === null) return { kind: 'unavailable', fight: target, pool: targetPool };
  if (target !== null && board?.halted !== true) {
    const mine = input.mine.get(target.id);
    if (mine === undefined) return { kind: 'unavailable', fight: target, pool: targetPool };
    // Always before "cannot add more" (RF42).
    if (wallet.balance < wallet.min_bet) {
      const placed = placedOn(mine) ? { fight: target, mine } : null;
      return { kind: 'broke', bailout: wallet.bailout, wallet, placed };
    }
    if (mine.max_stake === 0) return { kind: 'capped', fight: target, mine };
    const oneSided = targetPool === null || targetPool.a.stake === 0 || targetPool.b.stake === 0;
    const failure = input.failure;
    return {
      kind: 'open',
      fight: target,
      pool: targetPool,
      oneSided,
      wallet,
      mine,
      rejection: failure === null ? null : rejectionOf(failure.code, mine),
    };
  }
  if (wallet.balance < wallet.min_bet) {
    return { kind: 'broke', bailout: wallet.bailout, wallet, placed: null };
  }
  return { kind: 'none' };
}

function placedOf(input: PanelInput): Placed | null {
  const current = input.board?.current ?? null;
  if (!input.signedIn || current === null || current.state === 'betting_open') return null;
  if (current.state === 'scheduled' || current.state === 'settled') return null;
  const mine = input.mine.get(current.id);
  if (!placedOn(mine)) return null;
  return { fight: current, mine, halted: input.board?.halted === true };
}

export function panelState(input: PanelInput): PanelState {
  return {
    bonus:
      input.signedIn && input.wallet?.bonus_available === true ? input.wallet.bonus_amount : null,
    result: input.signedIn ? shownResultOf(input) : null,
    placed: placedOf(input),
    fight: fightZone(input),
  };
}
