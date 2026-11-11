// Reads the wardrobe files of content/models/ (022): `accessory.json` and `skin.json`. They are
// our own content, but a typo there must not reach the 3D layer as a half-valid object.
import { parseHex } from './color.ts';
import {
  BODY_SLOTS,
  type AccessoryAttach,
  type AccessoryFit,
  type AccessoryManifest,
  type BodyFit,
  type BodySlot,
  type ColorZone,
  type ColorZones,
  type SlotFit,
  type Vec3,
  type ZoneIsland,
} from './types.ts';

type Json = Readonly<Record<string, unknown>>;

const isObject = (value: unknown): value is Json =>
  typeof value === 'object' && value !== null && !Array.isArray(value);
const isNumber = (value: unknown): value is number =>
  typeof value === 'number' && Number.isFinite(value);
const isName = (value: unknown): value is string => typeof value === 'string' && value !== '';
const isBodySlot = (value: unknown): value is BodySlot =>
  (BODY_SLOTS as readonly unknown[]).includes(value);

function vec3(value: unknown): Vec3 | null {
  if (!Array.isArray(value) || value.length !== 3) return null;
  const [x, y, z]: unknown[] = value;
  return isNumber(x) && isNumber(y) && isNumber(z) ? [x, y, z] : null;
}

function attachOf(value: unknown): AccessoryAttach | null {
  if (!isObject(value) || !isName(value.bone)) return null;
  if (value.anchor !== undefined && !isName(value.anchor)) return null;
  if (value.mirror !== undefined && typeof value.mirror !== 'boolean') return null;
  if (value.node !== undefined && !isName(value.node)) return null;
  return {
    bone: value.bone,
    ...(value.anchor === undefined ? {} : { anchor: value.anchor }),
    ...(value.mirror === undefined ? {} : { mirror: value.mirror }),
    ...(value.node === undefined ? {} : { node: value.node }),
  };
}

function fitOf(value: unknown): AccessoryFit | null {
  if (!isObject(value)) return null;
  const out: Record<string, number> = {};
  for (const key of ['y', 'z', 'scale', 'tilt'] as const) {
    const field = value[key];
    if (field === undefined) continue;
    if (!isNumber(field)) return null;
    out[key] = field;
  }
  if (out.scale !== undefined && out.scale <= 0) return null;
  return out;
}

/** `null` when the manifest is not valid. */
export function parseAccessoryManifest(value: unknown): AccessoryManifest | null {
  if (!isObject(value) || !isBodySlot(value.slot) || !Array.isArray(value.attach)) return null;
  const attach = value.attach.map(attachOf);
  if (attach.length === 0 || attach.includes(null)) return null;
  const fit = value.fit === undefined ? undefined : fitOf(value.fit);
  if (fit === null) return null;
  if (value.hides !== undefined && typeof value.hides !== 'boolean') return null;
  return {
    slot: value.slot,
    attach: attach as AccessoryAttach[],
    ...(fit === undefined ? {} : { fit }),
    ...(value.hides === undefined ? {} : { hides: value.hides }),
  };
}

function bodyFitOf(value: unknown): BodyFit | null {
  if (!isObject(value)) return null;
  const out: Partial<Record<BodySlot, SlotFit>> = {};
  for (const [slot, entry] of Object.entries(value)) {
    if (!isBodySlot(slot) || !isObject(entry)) return null;
    const scale = vec3(entry.scale);
    const offset = vec3(entry.offset);
    if (scale === null || offset === null || scale.some((k) => k <= 0)) return null;
    out[slot] = { scale, offset };
  }
  return out;
}

function zoneOf(value: unknown): ColorZone | null {
  if (!isObject(value) || !isName(value.id) || parseHex(value.color) === null) return null;
  if (value.follows !== undefined && !isName(value.follows)) return null;
  if (value.only !== undefined && !(Array.isArray(value.only) && value.only.every(isName)))
    return null;
  return {
    id: value.id,
    color: value.color as string,
    ...(value.follows === undefined ? {} : { follows: value.follows }),
    ...(value.only === undefined ? {} : { only: value.only as string[] }),
  };
}

function islandOf(value: unknown): ZoneIsland | null {
  if (!isObject(value) || !isName(value.id) || !isName(value.zone)) return null;
  return { id: value.id, zone: value.zone };
}

/** What `skin.json` says, without the URLs the host adds. */
export interface SkinManifest {
  readonly fit?: BodyFit;
  readonly zones?: ColorZones;
  /** The gray image that numbers the islands: its file, the material it maps and the gray step. */
  readonly islandMap?: { readonly file: string; readonly material: string; readonly step: number };
}

/** `null` when the manifest is not valid. Every field is optional: a skin may be just its GLB. */
export function parseSkinManifest(value: unknown): SkinManifest | null {
  if (!isObject(value)) return null;
  const fit = value.fit === undefined ? undefined : bodyFitOf(value.fit);
  if (fit === null) return null;
  let zones: ColorZones | undefined;
  if (value.zones !== undefined) {
    if (!Array.isArray(value.zones)) return null;
    const list = value.zones.map(zoneOf);
    const islands = Array.isArray(value.islands) ? value.islands.map(islandOf) : [];
    if (list.includes(null) || islands.includes(null)) return null;
    zones = { zones: list as ColorZone[], islands: islands as ZoneIsland[] };
  }
  let islandMap: SkinManifest['islandMap'];
  if (value.island_map !== undefined) {
    const map = value.island_map;
    if (!isObject(map) || !isName(map.file) || !isName(map.material) || !isNumber(map.step))
      return null;
    if (map.step < 1 || /[\\/]/.test(map.file)) return null;
    islandMap = { file: map.file, material: map.material, step: map.step };
  }
  return {
    ...(fit === undefined ? {} : { fit }),
    ...(zones === undefined ? {} : { zones }),
    ...(islandMap === undefined ? {} : { islandMap }),
  };
}
