import type { BracketMatch, FriendTournament } from '@torneo/protocol';

// What a bracket shows (023 "Web"): its rounds in order, each with its matches from the top. Until
// the results are revealed, only round 1 says who fights: a later round would tell who advanced,
// and so would its replay's title, so its matches have no names nor links yet (011 RF20).

export interface BracketCell {
  readonly slot: number;
  /** Null while hidden. */
  readonly a: string | null;
  readonly b: string | null;
  /** Null while hidden. */
  readonly fightId: number | null;
  /** Who goes on; null while hidden. */
  readonly advances: string | null;
}

export interface BracketRound {
  readonly round: number;
  readonly final: boolean;
  readonly matches: readonly BracketCell[];
}

function cell(match: BracketMatch, shown: boolean, revealed: boolean): BracketCell {
  return {
    slot: match.slot,
    a: shown ? match.a : null,
    b: shown ? match.b : null,
    fightId: shown ? match.fight_id : null,
    advances: revealed ? (match.advances === 'a' ? match.a : match.b) : null,
  };
}

/** The rounds of `tournament`, with what may be seen of them. */
export function bracketRounds(
  tournament: Pick<FriendTournament, 'size' | 'matches'>,
  revealed: boolean,
): BracketRound[] {
  const total = Math.round(Math.log2(tournament.size));
  const rounds: BracketRound[] = [];
  for (let round = 1; round <= total; round += 1) {
    const matches = tournament.matches
      .filter((m) => m.round === round)
      .sort((x, y) => x.slot - y.slot)
      .map((m) => cell(m, revealed || round === 1, revealed));
    rounds.push({ round, final: round === total, matches });
  }
  return rounds;
}
