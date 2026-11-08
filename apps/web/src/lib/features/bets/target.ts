import type { FightSummary, FightsCurrent } from '@torneo/protocol';

// 012 RF42 and plan-c C1: the fight a bet goes to. `load` and the panel both use this, so the SSR
// reads the bets of the same fight the panel shows.

/** `current` if its window is open, else `next` if its window is, else none. */
export function targetFight(board: FightsCurrent | null): FightSummary | null {
  if (board === null) return null;
  if (board.current?.state === 'betting_open') return board.current;
  if (board.next?.state === 'betting_open') return board.next;
  return null;
}

/** The fights whose bets the page reads: the target, the `current` if it is another, and `last`. */
export function fightsToRead(board: FightsCurrent | null): number[] {
  const ids = [targetFight(board)?.id, board?.current?.id, board?.last?.id];
  return [...new Set(ids.filter((id): id is number => id !== undefined))];
}
