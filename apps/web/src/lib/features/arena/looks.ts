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
