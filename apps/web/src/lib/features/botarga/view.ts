import type { BotargaView, PendingDraw } from '@torneo/protocol';
import type { Locale } from '#lib/i18n/locale.js';
import {
  ACCESSORY_SLOTS,
  STAT_KEYS,
  STAT_MIN,
  STYLE_IDS,
  ability,
  abilityName,
  accessory,
  accessoryName,
  clipNumber,
  styleName,
  type AbilityKind,
  type AccessorySlot,
  type StatKey,
} from './catalog.js';

// What the components of the botarga draw, resolved by the page (ADR-0009 D4: a domain component
// receives its data by props). Names come from the catalog in the locale; clips go as a kind and a
// number, and the component words them.

/** A clip as the UI names it: "Idle 2", "Victory 1". */
export interface ClipLabel {
  readonly code: string;
  readonly clip: 'idle' | 'win';
  readonly n: number;
}

export interface BuildChoices {
  readonly statKeys: readonly StatKey[];
  readonly statMin: number;
  readonly styles: readonly { readonly id: string; readonly name: string }[];
  readonly abilities: readonly {
    readonly id: string;
    readonly name: string;
    readonly kind: AbilityKind | null;
  }[];
  readonly idles: readonly ClipLabel[];
  readonly victories: readonly ClipLabel[];
  /** The wardrobe: per slot, the accessories the user owns, with what each moves. */
  readonly wardrobe: readonly WardrobeSlot[];
}

export interface WardrobeOption {
  readonly id: string;
  readonly name: string;
  /** The stats it moves, in the order of the stats: zero-sum. */
  readonly deltas: readonly { readonly stat: StatKey; readonly delta: number }[];
}

export interface WardrobeSlot {
  readonly slot: AccessorySlot;
  readonly options: readonly WardrobeOption[];
}

function wardrobe(owned: readonly string[], locale: Locale): WardrobeSlot[] {
  return ACCESSORY_SLOTS.map((slot) => ({
    slot,
    options: owned.flatMap((id): WardrobeOption[] => {
      const item = accessory(id);
      if (item === null || item.slot !== slot) return [];
      const deltas = STAT_KEYS.flatMap((stat) => {
        const delta = item.deltas[stat] ?? 0;
        return delta === 0 ? [] : [{ stat, delta }];
      });
      return [{ id, name: accessoryName(id, locale), deltas }];
    }),
  }));
}

function clipLabel(code: string): ClipLabel | null {
  const n = clipNumber(code);
  if (n === null) return null;
  return { code, clip: code.startsWith('win_') ? 'win' : 'idle', n };
}

function clips(codes: readonly string[], kind: 'idle' | 'win'): ClipLabel[] {
  return codes
    .map(clipLabel)
    .filter((label): label is ClipLabel => label !== null && label.clip === kind);
}

/** What the build form offers: the user's own unlocks, named. */
export function buildChoices(botarga: BotargaView, locale: Locale): BuildChoices {
  return {
    statKeys: STAT_KEYS,
    statMin: STAT_MIN,
    styles: STYLE_IDS.map((id) => ({ id, name: styleName(id, locale) })),
    abilities: botarga.unlocks.abilities.map((id) => ({
      id,
      name: abilityName(id, locale),
      kind: ability(id)?.kind ?? null,
    })),
    idles: clips(botarga.unlocks.cosmetics, 'idle'),
    victories: clips(botarga.unlocks.cosmetics, 'win'),
    wardrobe: wardrobe(botarga.unlocks.accessories, locale),
  };
}

/** An option of a draw: an attack by its name, or a clip by its kind and number. */
export type DrawOption =
  | { readonly id: string; readonly kind: 'ability'; readonly name: string }
  | { readonly id: string; readonly kind: 'clip'; readonly clip: ClipLabel };

export function drawOptions(draw: PendingDraw, locale: Locale): DrawOption[] {
  return draw.options.flatMap((id): DrawOption[] => {
    if (draw.kind === 'ability') return [{ id, kind: 'ability', name: abilityName(id, locale) }];
    const clip = clipLabel(id);
    return clip === null ? [] : [{ id, kind: 'clip', clip }];
  });
}
