import type { FightSummary } from '@torneo/protocol';

// What the lists and the page show of a fight before anyone asks to watch it (011 RF17, RF18, RF20):
// who, when and, in the list, the kind of ending. Never a winner, a KO, a decision or a draw.

export type FightRowKind = 'played' | 'cancelled' | 'voided';

export interface FightRow {
  readonly id: number;
  readonly a: string;
  readonly b: string;
  /** Milliseconds since the epoch, as the API gives them. */
  readonly date: string;
  /** Null with a kind of ending this build does not know. */
  readonly kind: FightRowKind | null;
}

export function fightDate(fight: FightSummary): string {
  return fight.live_started_at ?? fight.live_at;
}

function rowKind(fight: FightSummary): FightRowKind | null {
  switch (fight.final?.kind) {
    case 'decided':
      return 'played';
    case 'cancelled':
      return 'cancelled';
    case 'voided':
      return 'voided';
    default:
      return null;
  }
}

export function fightRow(fight: FightSummary): FightRow {
  return {
    id: fight.id,
    a: fight.a.slug,
    b: fight.b.slug,
    date: fightDate(fight),
    kind: rowKind(fight),
  };
}
