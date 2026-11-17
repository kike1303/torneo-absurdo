import type {
  BotargaBuild,
  FighterCosmetics,
  IdleClip,
  ShopBuyRequest,
  ShopItem,
  ShopItemKind,
  ShopView,
  VictoryClip,
} from '@torneo/protocol';
import type { Locale } from '#lib/i18n/locale.js';
import {
  IDLE_CLIPS,
  STAT_KEYS,
  VICTORY_CLIPS,
  accessory,
  accessoryName,
  clipNumber,
  skinName,
  type StatKey,
} from '../botarga/catalog.js';
import type { ClipLabel } from '../botarga/view.js';

// The shop as the page draws it (024): the items by kind, each with its name and what stands in the
// way of buying it, the item picked to try on (`?item=<kind>:<id>`, so it works without JavaScript)
// and the body of the purchase. The server decides every rule and the price; this only shows what it
// answered, and the page sends back the price it showed.

const KINDS: readonly ShopItemKind[] = ['cosmetic', 'accessory', 'skin'];
const ITEM_ID = /^[a-z0-9_-]{1,64}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const PRICE = /^[1-9][0-9]{0,6}$/;
/** The levels of a tier of the catalog (`engine::loadout::LEVELS_PER_TIER`). */
const LEVELS_PER_TIER = 20;

/** What an item is called: a name of the catalog, or a clip the component words. */
export type ShopLabel =
  | { readonly kind: 'name'; readonly name: string }
  | { readonly kind: 'clip'; readonly clip: ClipLabel };

/** Why an item cannot be bought now, or `buyable`. */
export type ShopState = 'owned' | 'in_draw' | 'level' | 'buyable';

export interface ShopEntry {
  readonly kind: ShopItemKind;
  readonly id: string;
  /** Its value of `?item=`. */
  readonly param: string;
  readonly label: ShopLabel;
  readonly price: number;
  readonly state: ShopState;
  /** The level that opens its tier, for an accessory out of reach. */
  readonly level: number | null;
  /** What an accessory moves between stats (zero-sum); empty for the rest. */
  readonly deltas: readonly { readonly stat: StatKey; readonly delta: number }[];
}

export interface ShopSection {
  readonly kind: ShopItemKind;
  readonly entries: readonly ShopEntry[];
}

/** The first level whose tier is `tier` (`engine::loadout::tier`, the other way round). */
export function levelOfTier(tier: number): number {
  return Math.max(1, (tier - 1) * LEVELS_PER_TIER);
}

function label(item: Pick<ShopItem, 'kind' | 'id'>, locale: Locale): ShopLabel {
  if (item.kind === 'accessory') return { kind: 'name', name: accessoryName(item.id, locale) };
  if (item.kind === 'skin') return { kind: 'name', name: skinName(item.id, locale) };
  const n = clipNumber(item.id);
  if (n === null) return { kind: 'name', name: item.id };
  return {
    kind: 'clip',
    clip: { code: item.id, clip: item.id.startsWith('win_') ? 'win' : 'idle', n },
  };
}

function state(item: ShopItem): ShopState {
  if (item.owned) return 'owned';
  if (item.in_draw) return 'in_draw';
  return item.level_ok ? 'buyable' : 'level';
}

function entry(item: ShopItem, locale: Locale): ShopEntry {
  const found = item.kind === 'accessory' ? accessory(item.id) : null;
  return {
    kind: item.kind,
    id: item.id,
    param: itemParam(item),
    label: label(item, locale),
    price: item.price,
    state: state(item),
    level: !item.level_ok && item.tier !== null ? levelOfTier(item.tier) : null,
    deltas:
      found === null
        ? []
        : STAT_KEYS.flatMap((stat) => {
            const delta = found.deltas[stat] ?? 0;
            return delta === 0 ? [] : [{ stat, delta }];
          }),
  };
}

/** The items by kind, in the order of the catalog; a kind without items has no section. */
export function shopSections(view: ShopView, locale: Locale): ShopSection[] {
  return KINDS.flatMap((kind) => {
    const entries = view.items.filter((item) => item.kind === kind).map((i) => entry(i, locale));
    return entries.length === 0 ? [] : [{ kind, entries }];
  });
}

/** `?item=<kind>:<id>`, the item picked; `null` for anything else. */
export function parseItemParam(raw: string | null): { kind: ShopItemKind; id: string } | null {
  if (raw === null) return null;
  const at = raw.indexOf(':');
  const kind = raw.slice(0, at) as ShopItemKind;
  const id = raw.slice(at + 1);
  return at > 0 && KINDS.includes(kind) && ITEM_ID.test(id) ? { kind, id } : null;
}

export function itemParam(item: { kind: ShopItemKind; id: string }): string {
  return `${item.kind}:${item.id}`;
}

/** What the preview wears: the saved build with the picked item put on. */
export interface TriedLook {
  readonly cosmetics: FighterCosmetics;
  readonly wear: readonly string[];
  readonly skin: string | null;
}

export function tryOn(
  build: Pick<BotargaBuild, 'cosmetics' | 'accessories' | 'skin'>,
  item: { kind: ShopItemKind; id: string } | null,
): TriedLook {
  const look: TriedLook = {
    cosmetics: build.cosmetics,
    wear: build.accessories,
    skin: build.skin,
  };
  if (item === null) return look;
  if (item.kind === 'skin') return { ...look, skin: item.id };
  if (item.kind === 'cosmetic') {
    if ((IDLE_CLIPS as readonly string[]).includes(item.id))
      return { ...look, cosmetics: { ...look.cosmetics, idle: item.id as IdleClip } };
    if ((VICTORY_CLIPS as readonly string[]).includes(item.id))
      return { ...look, cosmetics: { ...look.cosmetics, victory: item.id as VictoryClip } };
    return look;
  }
  // An accessory takes the place of the one in its slot.
  const slot = accessory(item.id)?.slot;
  if (slot === undefined) return look;
  const others = look.wear.filter((id) => accessory(id)?.slot !== slot);
  return { ...look, wear: [...others, item.id] };
}

function text(form: FormData, name: string): string {
  const value = form.get(name);
  return typeof value === 'string' ? value : '';
}

/** The body of `POST /api/shop/buy` from its form; `null` if malformed. */
export function parseBuyForm(form: FormData): ShopBuyRequest | null {
  const kind = text(form, 'kind') as ShopItemKind;
  const id = text(form, 'item_id');
  const price = text(form, 'expected_price');
  const key = text(form, 'client_key');
  if (!KINDS.includes(kind) || !ITEM_ID.test(id) || !PRICE.test(price) || !UUID.test(key)) {
    return null;
  }
  return { kind, item_id: id, expected_price: Number(price), client_key: key };
}
