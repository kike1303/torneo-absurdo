// Names of the wardrobe pieces on /dev/arena (TA-76). The accessories are catalog content, already
// translated in content/catalog/names.<locale>.json; an id without a name shows as itself, as a
// local fighter does (it is data, not a UI string).
import namesEn from '../../../../../../content/catalog/names.en.json';
import namesEs from '../../../../../../content/catalog/names.es.json';
import { getLocale } from '#lib/paraglide/runtime.js';

const ACCESSORY_NAMES: Record<string, Readonly<Record<string, string>>> = {
  en: namesEn.accessories,
  es: namesEs.accessories,
};

export function accessoryName(id: string): string {
  return ACCESSORY_NAMES[getLocale()]?.[id] ?? id;
}

/** Skins have no catalog names yet: the folder slug. */
export function skinName(slug: string): string {
  return slug;
}
