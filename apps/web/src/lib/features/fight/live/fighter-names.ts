import lore from 'virtual:fighter-lore';
import { m } from '#lib/paraglide/messages.js';
import type { Locale } from '#lib/i18n/locale.js';

// Who fights, by slug (010 RF56). A slug without lore is valid: the characters of the e2e.

export interface FighterDisplay {
  readonly name: string;
  readonly epithet: string | null;
}

export function fighterDisplay(slug: string, locale: Locale): FighterDisplay {
  const entry = Object.hasOwn(lore, slug) ? lore[slug]?.[locale] : undefined;
  return entry === undefined
    ? { name: m.fight_name_unknown({ slug }, { locale }), epithet: null }
    : { name: entry.name, epithet: entry.epithet };
}
