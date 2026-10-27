// The owner's map (002 RF-B2): which MUGEN action is each of our animations.
import {
  ATTACK_ANIMATIONS,
  LIMITS,
  OPTIONAL_ANIMATIONS,
  REQUIRED_ANIMATIONS,
  type AnimationCode,
  type AttackAnimation,
} from '@torneo/renderer/manifest';
import { fail } from './errors.ts';

export interface CharacterMap {
  readonly id: string;
  readonly actions: Readonly<Partial<Record<AnimationCode, number>>>;
  readonly impactFrames: Readonly<Record<AttackAnimation, number>>;
  readonly scale: number;
  /** SFF v1 with a shared palette: the sprite (group, image) that holds the palette. */
  readonly paletteSprite: readonly [number, number];
}

/** Standard MUGEN actions (spec RF-B2). */
export const DEFAULT_ACTIONS: Readonly<Partial<Record<AnimationCode, number>>> = Object.freeze({
  idle: 0,
  walk: 20,
  hit: 5000,
  knockdown: 5050,
  intro: 190,
  win: 180,
});

const FIELDS = ['id', 'actions', 'impact_frames', 'scale', 'palette_sprite'];
const CODES: readonly string[] = [...REQUIRED_ANIMATIONS, ...OPTIONAL_ANIMATIONS];
const ATTACKS: readonly string[] = ATTACK_ANIMATIONS;

function isObject(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function isInt(value: unknown, min: number, max: number): value is number {
  return typeof value === 'number' && Number.isInteger(value) && value >= min && value <= max;
}

/**
 * Validates the parsed JSON of the map. `JSON.parse` happens in cli.ts, the only module besides
 * png.ts allowed a `catch` (plan-b decision 3); its size is checked there with `fs.stat`.
 */
export function parseMap(json: unknown): CharacterMap {
  if (!isObject(json)) return fail('map.not_object');
  for (const key of Object.keys(json)) {
    if (!FIELDS.includes(key)) fail('map.unknown_field', { field: key });
  }
  const id = json['id'];
  if (typeof id !== 'string' || !LIMITS.idPattern.test(id)) return fail('map.invalid_id');

  const actions: Partial<Record<AnimationCode, number>> = { ...DEFAULT_ACTIONS };
  const rawActions = json['actions'] ?? {};
  if (!isObject(rawActions)) return fail('map.invalid_field', { field: 'actions' });
  for (const [key, value] of Object.entries(rawActions)) {
    if (!CODES.includes(key)) fail('map.unknown_field', { field: `actions.${key}` });
    if (!isInt(value, -1, 2_147_483_647)) fail('map.invalid_action', { animation: key });
    actions[key as AnimationCode] = value;
  }
  for (const code of REQUIRED_ANIMATIONS) {
    if (actions[code] === undefined) fail('map.missing_action', { animation: code });
  }

  const rawImpacts = json['impact_frames'] ?? {};
  if (!isObject(rawImpacts)) return fail('map.invalid_field', { field: 'impact_frames' });
  for (const key of Object.keys(rawImpacts)) {
    if (!ATTACKS.includes(key)) fail('map.unknown_field', { field: `impact_frames.${key}` });
  }
  const impacts: Partial<Record<AttackAnimation, number>> = {};
  for (const code of ATTACK_ANIMATIONS) {
    const value = rawImpacts[code];
    if (value === undefined) fail('map.missing_impact_frame', { animation: code });
    if (!isInt(value, 0, LIMITS.animationFramesMax - 1)) {
      fail('map.invalid_impact_frame', { animation: code });
    }
    impacts[code] = value;
  }

  const scale = json['scale'] ?? 1;
  if (
    typeof scale !== 'number' ||
    !Number.isFinite(scale) ||
    scale < LIMITS.scaleMin ||
    scale > LIMITS.scaleMax
  ) {
    return fail('map.scale_out_of_range');
  }

  const rawPalette = json['palette_sprite'] ?? [0, 0];
  if (
    !Array.isArray(rawPalette) ||
    rawPalette.length !== 2 ||
    !isInt(rawPalette[0], 0, 65535) ||
    !isInt(rawPalette[1], 0, 65535)
  ) {
    return fail('map.invalid_field', { field: 'palette_sprite' });
  }

  return {
    id,
    actions,
    impactFrames: impacts as Record<AttackAnimation, number>,
    scale,
    paletteSprite: [rawPalette[0], rawPalette[1]],
  };
}
