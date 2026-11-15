import type { PvpFightDetail, PvpFighter, PvpFightSummary, PvpLook } from '@torneo/protocol';
import type { Locale } from '#lib/i18n/locale.js';
import { fighterDisplay } from '../fight/live/fighter-names.js';

// The name of a side of a player fight (021): a user by their public `display_name`, a character of
// the tournament by its lore.

export function pvpFighterName(fighter: PvpFighter, locale: Locale): string {
  return fighter.kind === 'player'
    ? fighter.display_name
    : fighterDisplay(fighter.slug, locale).name;
}

/** A row of my fights with the name of the other side, resolved by the page. */
export interface PvpFightRow {
  readonly fight: PvpFightSummary;
  readonly opponent: string;
}

export function pvpFightRows(fights: readonly PvpFightSummary[], locale: Locale): PvpFightRow[] {
  return fights.map((fight) => ({ fight, opponent: pvpFighterName(fight.opponent, locale) }));
}

/** What the stage draws of a side: its idle and its victory, and the accessories it wears. */
export interface StageLooks {
  readonly cosmetics: Partial<
    Record<
      'a' | 'b',
      { idle?: NonNullable<PvpLook['idle']>; victory?: NonNullable<PvpLook['victory']> }
    >
  >;
  readonly looks: Partial<Record<'a' | 'b', { wear: readonly string[] }>>;
}

/**
 * The looks of a player fight as the stage reads them (TA-67, TA-84). A side without a look (a
 * character of the tournament, an old fight) keeps the tournament ones.
 */
export function stageLooks(detail: Pick<PvpFightDetail, 'look_a' | 'look_b'>): StageLooks {
  const cosmetics: StageLooks['cosmetics'] = {};
  const looks: StageLooks['looks'] = {};
  for (const [side, look] of [
    ['a', detail.look_a],
    ['b', detail.look_b],
  ] as const) {
    if (look === null) continue;
    cosmetics[side] = {
      ...(look.idle === null ? {} : { idle: look.idle }),
      ...(look.victory === null ? {} : { victory: look.victory }),
    };
    if (look.accessories.length > 0) looks[side] = { wear: look.accessories };
  }
  return { cosmetics, looks };
}
