import type { PvpFighter, PvpFightSummary } from '@torneo/protocol';
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
