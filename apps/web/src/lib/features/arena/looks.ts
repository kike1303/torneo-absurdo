// The wardrobe of /dev/arena (022), from the URL: `?wear_a=propeller_beanie,oven_mitts`,
// `?skin_b=<folder>` and `?colors_a=primary:ff0000,z1:00aa88`, and the same with the other side.
// Only the shape is checked here; an id that does not exist in content/models/ wears nothing.
import type { Side } from '@torneo/renderer';
import type { LookSpec, LookSpecs } from '#lib/components/fight/botarga-model.js';

const ID = /^[a-z][a-z0-9_]{0,47}$/;
const COLOR = /^([a-z][a-z0-9_]{0,15}):([0-9a-f]{6})$/i;
const MAX_ITEMS = 8;

type Params = Pick<URLSearchParams, 'get'>;

function list(raw: string | null): string[] {
  return raw === null ? [] : raw.split(',').slice(0, MAX_ITEMS);
}

function parseLook(params: Params, side: Side): LookSpec | undefined {
  const skin = params.get(`skin_${side}`);
  const wear = list(params.get(`wear_${side}`)).filter((id) => ID.test(id));
  const colors = Object.fromEntries(
    list(params.get(`colors_${side}`)).flatMap((entry) => {
      const match = COLOR.exec(entry);
      return match === null ? [] : [[match[1], `#${match[2]}`]];
    }),
  );
  const validSkin = skin !== null && ID.test(skin) ? skin : undefined;
  if (validSkin === undefined && wear.length === 0 && Object.keys(colors).length === 0)
    return undefined;
  return { skin: validSkin, wear, colors };
}

/** `undefined` without any wardrobe parameter: the stage then loads nothing extra. */
export function parseLooks(params: Params): LookSpecs | undefined {
  const a = parseLook(params, 'a');
  const b = parseLook(params, 'b');
  return a === undefined && b === undefined ? undefined : { a, b };
}

/** The color slots the arena controls offer (those of the accessories); a skin zone stays URL-only. */
export const COLOR_SLOTS = ['primary', 'secondary', 'accent'] as const;
export type ColorSlot = (typeof COLOR_SLOTS)[number];

const WARDROBE_KEYS = ['wear', 'skin', 'colors'] as const;
const SIDES: readonly Side[] = ['a', 'b'];

/**
 * The inverse of `parseLooks`: a copy of `params` with the wardrobe of `looks` and every other
 * parameter untouched. An empty part drops its key, so an empty look leaves a clean URL.
 */
export function writeLooks(params: URLSearchParams, looks: LookSpecs | undefined): URLSearchParams {
  const next = new URLSearchParams(params);
  for (const side of SIDES) {
    for (const key of WARDROBE_KEYS) next.delete(`${key}_${side}`);
    const look = looks?.[side];
    if (look === undefined) continue;
    const wear = [...new Set(look.wear ?? [])].filter((id) => ID.test(id)).slice(0, MAX_ITEMS);
    if (wear.length > 0) next.set(`wear_${side}`, wear.join(','));
    if (look.skin !== undefined && ID.test(look.skin)) next.set(`skin_${side}`, look.skin);
    const colors = Object.entries(look.colors ?? {})
      .map(([slot, color]) => `${slot}:${color.replace(/^#/, '').toLowerCase()}`)
      .filter((entry) => COLOR.test(entry))
      .slice(0, MAX_ITEMS);
    if (colors.length > 0) next.set(`colors_${side}`, colors.join(','));
  }
  return next;
}

/** The wardrobe part of the query, stable: the key that remounts the stage when it changes. */
export function looksKey(params: Params): string {
  return SIDES.flatMap((side) =>
    WARDROBE_KEYS.map((key) => params.get(`${key}_${side}`) ?? ''),
  ).join('|');
}
