// The pose of a 3D fighter as a pure function of its `FighterScene` (013): which clip of the
// botarga, at which clip time, and which face. Pure: the Three layer only applies it, so `seek`
// shows the same as playing up to there and every spectator sees the same variant.
import type { AnimationCode } from '../manifest/types.ts';
import type { FighterScene } from '../scene/types.ts';
import type { Side } from '../timeline/types.ts';

/** The 16 clips of `content/models/botarga-base/botarga-base.glb`. */
export const CLIP_NAMES = [
  'idle',
  'attack_light',
  'attack_heavy_1',
  'attack_heavy_2',
  'special_1',
  'special_2',
  'special_3',
  'special_4',
  'hit',
  'block',
  'dodge',
  'ko_1',
  'ko_2',
  'win_1',
  'win_2',
  'win_3',
] as const;
export type ClipName = (typeof CLIP_NAMES)[number];

/**
 * Where each attack clip lands, as a fraction of its length: the frame where a wrist reaches
 * furthest forward of the hips (measured on the GLB, 013).
 */
export const CLIP_IMPACT: Readonly<Partial<Record<ClipName, number>>> = Object.freeze({
  attack_light: 0.29,
  attack_heavy_1: 0.41,
  attack_heavy_2: 0.16,
  special_1: 0.16,
  special_2: 0.14,
  special_3: 0.2,
  special_4: 0.2,
});

const VARIANTS: Readonly<Record<AnimationCode, readonly ClipName[]>> = Object.freeze({
  idle: ['idle'],
  intro: ['idle'],
  walk: ['idle'],
  attack_light: ['attack_light'],
  attack_heavy: ['attack_heavy_1', 'attack_heavy_2'],
  special: ['special_1', 'special_2', 'special_3', 'special_4'],
  hit: ['hit'],
  knockdown: ['hit'],
  block: ['block'],
  dodge: ['dodge'],
  ko: ['ko_1', 'ko_2'],
  win: ['win_1', 'win_2', 'win_3'],
});

/** Clips that play at their own speed and hold the last frame (the ending, 013). */
const HELD: ReadonlySet<AnimationCode> = new Set(['ko', 'win']);

export const FACE_CODES = [
  'neutral',
  'guardia',
  'preparacion',
  'golpe',
  'dano',
  'ko',
  'aturdido',
  'victoria',
  'derrota',
  'celebracion',
  'concentrado',
  'alerta',
  'vigilando',
  'cansado',
  'recuperandose',
  'enfadado',
  'decidido',
  'entusiasmado',
  'dudoso',
  'dormido',
] as const;
export type FaceCode = (typeof FACE_CODES)[number];

/** Below this fraction of the max HP, the idle face is tired. */
export const TIRED_HP = 0.25;

/** Seconds of each clip, read from the GLB by the Three layer. */
export type ClipDurations = Readonly<Record<ClipName, number>>;

export interface Pose {
  readonly clip: ClipName;
  /** Seconds into the clip, in [0, duration]. */
  readonly time: number;
  readonly face: FaceCode;
}

/** FNV-1a over the side and the start of the animation: the same variant on every client. */
function variantIndex(side: Side, start: number, count: number): number {
  let h = 2166136261;
  for (const v of [side === 'a' ? 1 : 2, Math.round(start)]) {
    for (let shift = 0; shift < 32; shift += 8) {
      h ^= (v >>> shift) & 255;
      h = Math.imul(h, 16777619) >>> 0;
    }
  }
  return h % count;
}

/** The clip of `fighter` at scene time `t`; the variant depends only on when it started. */
export function clipFor(
  fighter: Pick<FighterScene, 'side' | 'animation' | 'elapsed'>,
  t: number,
): ClipName {
  const variants = VARIANTS[fighter.animation];
  if (variants.length === 1) return variants[0] as ClipName;
  return variants[variantIndex(fighter.side, t - fighter.elapsed, variants.length)] as ClipName;
}

/**
 * Seconds into `clip` for `elapsed` ms of the sprite animation. Attacks are retimed piecewise so
 * the clip's impact lands on the sprite's impact (`windup`); reactions stretch to the sprite's
 * length; `ko` and `win` run at their own speed and hold. Idle loops on the global phase of `t`,
 * so it never jumps when a fighter goes from contact or a walk back to its mark.
 */
export function clipTime(
  fighter: Pick<FighterScene, 'animation' | 'elapsed' | 'windup' | 'total'>,
  clip: ClipName,
  duration: number,
  t: number,
): number {
  const ms = Math.max(0, fighter.elapsed);
  if (fighter.animation === 'idle' || fighter.animation === 'intro' || fighter.animation === 'walk')
    return duration > 0 ? (Math.max(0, t) / 1000) % duration : 0;
  if (HELD.has(fighter.animation)) return Math.min(duration, ms / 1000);
  const total = Math.max(1, fighter.total);
  const impact = CLIP_IMPACT[clip];
  if (impact !== undefined && fighter.windup > 0 && fighter.windup < total) {
    const hit = impact * duration;
    if (ms <= fighter.windup) return (ms / fighter.windup) * hit;
    const p = Math.min(1, (ms - fighter.windup) / (total - fighter.windup));
    return hit + p * (duration - hit);
  }
  return Math.min(1, ms / total) * duration;
}

export interface FaceContext {
  readonly hpFraction: number;
  /** The fight has not started yet (phase `intro`). */
  readonly intro: boolean;
}

export function faceFor(
  fighter: Pick<FighterScene, 'animation' | 'elapsed' | 'windup'>,
  context: FaceContext,
): FaceCode {
  switch (fighter.animation) {
    case 'attack_light':
    case 'attack_heavy':
      return fighter.elapsed < fighter.windup ? 'preparacion' : 'golpe';
    case 'special':
      return 'enfadado';
    case 'hit':
    case 'knockdown':
      return 'dano';
    case 'block':
      return 'concentrado';
    case 'dodge':
      return 'alerta';
    case 'ko':
      return 'ko';
    case 'win':
      return 'victoria';
    default:
      if (context.intro) return 'neutral';
      return context.hpFraction < TIRED_HP ? 'cansado' : 'guardia';
  }
}

/** The pose of `fighter` at scene time `t`, given the length of each clip. */
export function poseFor(
  fighter: FighterScene,
  t: number,
  durations: ClipDurations,
  context: FaceContext,
): Pose {
  const clip = clipFor(fighter, t);
  return {
    clip,
    time: clipTime(fighter, clip, durations[clip], t),
    face: faceFor(fighter, context),
  };
}
