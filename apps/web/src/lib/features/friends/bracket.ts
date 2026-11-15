import type { FriendTournament, Side } from '@torneo/protocol';

// What a bracket shows (023 "Web"). Until the results are revealed only round 1 says who fights:
// a later round would tell who advanced, and so would its replay's title (011 RF20). The page's
// HTML carries only that (`hiddenView`); the whole tournament is read by the browser when the
// reader asks for the results, as a replay reads its fight.

/** One match as a view has it: `advances` only once revealed. */
export interface ViewMatch {
  readonly round: number;
  readonly slot: number;
  readonly a: string;
  readonly b: string;
  readonly fight_id: number;
  readonly advances?: Side;
}

/** What a bracket page may carry before the results: who plays and round 1, no outcome. */
export interface HiddenTournament {
  readonly id: number;
  readonly size: number;
  readonly owner: string;
  readonly players: readonly string[];
  readonly matches: readonly ViewMatch[];
}

/** The tournament cut to what may be seen before the results. */
export function hiddenView(tournament: FriendTournament): HiddenTournament {
  return {
    id: tournament.id,
    size: tournament.size,
    owner: tournament.owner,
    players: tournament.players,
    matches: tournament.matches
      .filter((m) => m.round === 1)
      .map(({ round, slot, a, b, fight_id }) => ({ round, slot, a, b, fight_id })),
  };
}

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

/** The rounds of a tournament of `size`, each with its places from the top; a place whose match
 * is not in `matches` (or not shown yet) is "to be decided". */
export function bracketRounds(
  view: { readonly size: number; readonly matches: readonly ViewMatch[] },
  revealed: boolean,
): BracketRound[] {
  const total = Math.round(Math.log2(view.size));
  const rounds: BracketRound[] = [];
  for (let round = 1; round <= total; round += 1) {
    const places = view.size / 2 ** round;
    const matches: BracketCell[] = [];
    for (let slot = 0; slot < places; slot += 1) {
      const match = view.matches.find((m) => m.round === round && m.slot === slot);
      const shown = match !== undefined && (revealed || round === 1);
      const side = revealed ? match?.advances : undefined;
      matches.push({
        slot,
        a: shown ? match.a : null,
        b: shown ? match.b : null,
        fightId: shown ? match.fight_id : null,
        advances: side === undefined || match === undefined ? null : match[side],
      });
    }
    rounds.push({ round, final: round === total, matches });
  }
  return rounds;
}

/** The body of `GET /api/friends/tournaments/{id}` if it is tournament `id`; otherwise null. */
export function parseTournament(value: unknown, id: number): FriendTournament | null {
  if (typeof value !== 'object' || value === null) return null;
  const t = value as Partial<FriendTournament>;
  const shaped =
    t.id === id &&
    typeof t.size === 'number' &&
    typeof t.owner === 'string' &&
    typeof t.champion === 'string' &&
    Array.isArray(t.players) &&
    Array.isArray(t.matches) &&
    t.matches.every(
      (m) =>
        typeof m.round === 'number' &&
        typeof m.slot === 'number' &&
        typeof m.a === 'string' &&
        typeof m.b === 'string' &&
        typeof m.fight_id === 'number' &&
        (m.advances === 'a' || m.advances === 'b'),
    );
  return shaped ? (t as FriendTournament) : null;
}
