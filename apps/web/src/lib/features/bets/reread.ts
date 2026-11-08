// 012 RF36, plan-c C5: the payout reaches the page by reading again. When the view shows the result
// of a fight the user bet on, and after a random wait of 0 to 3 s (everyone sees that result at
// once), the page reads its wallet and its bets again. Once per fight. The same scheduler reads the
// bets of a fight whose window opened after the page loaded.

export const MAX_REREAD_DELAY_MS = 3000;

export interface RereadDeps {
  readonly random: () => number;
  readonly setTimer: (run: () => void, ms: number) => unknown;
  readonly reread: () => void;
}

export interface Reread {
  /** A read is due for `fightId` if `needed`: once per fight, after the random wait. */
  due(fightId: number, needed: boolean): void;
}

export function createReread(deps: RereadDeps): Reread {
  const done = new Set<number>();
  return {
    due(fightId, needed) {
      if (!needed || done.has(fightId)) return;
      done.add(fightId);
      deps.setTimer(deps.reread, Math.floor(deps.random() * MAX_REREAD_DELAY_MS));
    },
  };
}
