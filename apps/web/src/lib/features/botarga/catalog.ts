// The catalog of the player botarga as the UI reads it (016, 021): what each attack is and how it
// is called. The server embeds the same files and decides; this only names and groups. The names
// are catalog content already translated in content/catalog/names.<locale>.json; an id without a
// name shows as itself (data, not a UI string), as `wardrobe-name.ts` does.
import abilitiesFile from '../../../../../../content/catalog/abilities.json';
import accessoriesFile from '../../../../../../content/catalog/accessories.json';
import namesEn from '../../../../../../content/catalog/names.en.json';
import namesEs from '../../../../../../content/catalog/names.es.json';
import stylesFile from '../../../../../../content/catalog/styles.json';
import type { Locale } from '#lib/i18n/locale.js';

export type AbilityKind = 'heavy' | 'special';

export interface CatalogAbility {
  readonly id: string;
  readonly kind: AbilityKind;
  readonly tier: number;
  readonly element: string | null;
}

const NAMES: Record<Locale, typeof namesEn> = { en: namesEn, es: namesEs };

const ABILITIES: ReadonlyMap<string, CatalogAbility> = new Map(
  abilitiesFile.abilities.map((a) => [
    a.id,
    { id: a.id, kind: a.kind as AbilityKind, tier: a.tier, element: a.element },
  ]),
);

/** The clothing slots of an accessory (016), in the order the wardrobe shows them. */
export const ACCESSORY_SLOTS = ['head', 'hands', 'torso', 'legs', 'feet'] as const;
export type AccessorySlot = (typeof ACCESSORY_SLOTS)[number];

export interface CatalogAccessory {
  readonly id: string;
  readonly slot: AccessorySlot;
  /** What it moves between stats: zero-sum (016). Only the stats it changes. */
  readonly deltas: Readonly<Partial<Record<StatKey, number>>>;
}

const ACCESSORIES: ReadonlyMap<string, CatalogAccessory> = new Map(
  accessoriesFile.accessories.map((a) => [
    a.id,
    {
      id: a.id,
      slot: a.slot as AccessorySlot,
      deltas: a.deltas as Partial<Record<StatKey, number>>,
    },
  ]),
);

export function accessory(id: string): CatalogAccessory | null {
  return ACCESSORIES.get(id) ?? null;
}

export function accessoryName(id: string, locale: Locale): string {
  return (NAMES[locale].accessories as Record<string, string>)[id] ?? id;
}

/** The styles of the catalog, in its order. */
export const STYLE_IDS: readonly string[] = stylesFile.styles.map((s) => s.id);

/** The idle and victory clip codes a botarga may pick (016); `null` is the tournament variant. */
export const IDLE_CLIPS = ['idle', 'idle_2', 'idle_3', 'idle_4', 'idle_5'] as const;
export const VICTORY_CLIPS = ['win_1', 'win_2', 'win_3'] as const;

/** The stats of a build, in the order of the protocol. */
export const STAT_KEYS = ['hp', 'attack', 'defense', 'speed', 'special'] as const;
export type StatKey = (typeof STAT_KEYS)[number];
/** The minimum of each stat in the player ruleset (v2). */
export const STAT_MIN = 20;

export function ability(id: string): CatalogAbility | null {
  return ABILITIES.get(id) ?? null;
}

export function abilityName(id: string, locale: Locale): string {
  return (NAMES[locale].abilities as Record<string, string>)[id] ?? id;
}

export function styleName(id: string, locale: Locale): string {
  return (NAMES[locale].styles as Record<string, string>)[id] ?? id;
}

/** A suit of the shop (024) by its slug; one without a name shows as its slug. */
export function skinName(slug: string, locale: Locale): string {
  return (NAMES[locale].skins as Record<string, string>)[slug] ?? slug;
}

/** Whether `code` is one of the clip codes a draw of cosmetics offers. */
export function isClip(code: string): boolean {
  return (
    (IDLE_CLIPS as readonly string[]).includes(code) ||
    (VICTORY_CLIPS as readonly string[]).includes(code)
  );
}

/** The number a clip code shows as (`idle` is 1, `idle_3` is 3, `win_2` is 2), or null. */
export function clipNumber(code: string): number | null {
  if (code === 'idle') return 1;
  const match = /^(?:idle|win)_([1-5])$/.exec(code);
  return match === null ? null : Number(match[1]);
}
