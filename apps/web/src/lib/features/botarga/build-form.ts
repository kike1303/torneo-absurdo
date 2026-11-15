import type { BotargaBuild, FighterCosmetics, IdleClip, VictoryClip } from '@torneo/protocol';
import { IDLE_CLIPS, STAT_KEYS, VICTORY_CLIPS } from './catalog.js';

// The form of the build (021 "Build"): what the browser sends, read into the body of
// `PUT /api/botarga/build`. Only the shape is checked here; the server checks the unlocks, the
// budget and the rules of the loadout, and answers with a code. Anything malformed is `null`, and
// the action answers `botarga.invalid_request` without asking the server.

const MAX_ID_BYTES = 64;
const MAX_ABILITIES = 4;
const MAX_ACCESSORIES = 5;
const INTEGER = /^(?:0|[1-9][0-9]{0,4})$/;

const isId = (value: string): boolean =>
  value.length > 0 && new TextEncoder().encode(value).length <= MAX_ID_BYTES;

function text(form: FormData, name: string): string | null {
  const value = form.get(name);
  return typeof value === 'string' ? value : null;
}

function texts(form: FormData, name: string): string[] | null {
  const values = form.getAll(name);
  return values.every((v) => typeof v === 'string') ? (values as string[]) : null;
}

function clip<T extends string>(value: string | null, allowed: readonly T[]): T | null | undefined {
  if (value === null || value === '') return null;
  return (allowed as readonly string[]).includes(value) ? (value as T) : undefined;
}

export function parseBuildForm(form: FormData): BotargaBuild | null {
  const stats: Record<string, number> = {};
  for (const key of STAT_KEYS) {
    const value = text(form, key)?.trim() ?? '';
    if (!INTEGER.test(value)) return null;
    stats[key] = Number(value);
  }
  const style = text(form, 'style_id');
  const abilities = texts(form, 'ability');
  // A slot left on "none" sends an empty value.
  const accessories = texts(form, 'accessory')?.filter((id) => id !== '') ?? null;
  if (style === null || !isId(style) || abilities === null || accessories === null) return null;
  if (abilities.length > MAX_ABILITIES || accessories.length > MAX_ACCESSORIES) return null;
  if (![...abilities, ...accessories].every(isId)) return null;
  const idle = clip<IdleClip>(text(form, 'idle'), IDLE_CLIPS);
  const victory = clip<VictoryClip>(text(form, 'victory'), VICTORY_CLIPS);
  if (idle === undefined || victory === undefined) return null;
  const cosmetics: FighterCosmetics = { idle, victory };
  return {
    stats: {
      hp: stats.hp ?? 0,
      attack: stats.attack ?? 0,
      defense: stats.defense ?? 0,
      speed: stats.speed ?? 0,
      special: stats.special ?? 0,
    },
    style_id: style,
    abilities,
    accessories,
    cosmetics,
  };
}

/** The body of `POST /api/botarga/draw/choose` from its form; `null` if malformed. */
export function parseDrawForm(form: FormData): { draw_id: number; item_id: string } | null {
  const id = text(form, 'draw_id') ?? '';
  const item = text(form, 'item_id') ?? '';
  if (!/^[1-9][0-9]{0,9}$/.test(id) || !isId(item)) return null;
  const drawId = Number(id);
  return drawId <= 2_147_483_647 ? { draw_id: drawId, item_id: item } : null;
}

/** A client key of the form, canonical lowercase UUID, or `null`. */
export function parseClientKey(form: FormData): string | null {
  const key = text(form, 'client_key') ?? '';
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(key) ? key : null;
}
