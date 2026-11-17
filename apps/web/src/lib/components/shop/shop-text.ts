import type { ShopLabel } from '#lib/features/shop/shop.js';
import { formatInteger } from '#lib/i18n/format.js';
import type { Locale } from '#lib/i18n/locale.js';
import { m } from '#lib/paraglide/messages.js';

/** The name of an item: of the catalog, or a clip worded ("Idle 2", "Victory 1"). */
export function labelText(label: ShopLabel, locale: Locale): string {
  if (label.kind === 'name') return label.name;
  const n = formatInteger(label.clip.n, locale);
  return label.clip.clip === 'win' ? m.botarga_clip_victory({ n }) : m.botarga_clip_idle({ n });
}
