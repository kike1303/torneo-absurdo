// Pure description of the scene at an instant (RF-A9). No colors, only roles (plan-a1
// decision 3); no text, only codes (ADR-0007).
import type { AnimationCode } from '../manifest/types.ts';
import type { Ending, Side } from '../timeline/types.ts';

/** Logical stage (RF-A15). Fighters, effects and damage numbers live in these coordinates. */
export const STAGE_WIDTH = 640;
export const STAGE_HEIGHT = 360;
/** Where the feet (the frame pivot) stand. */
export const GROUND_Y = 320;
/** Standing marks at 1/3 and 2/3 of the logical width. */
export const MARK_X: Readonly<Record<Side, number>> = Object.freeze({
  a: STAGE_WIDTH / 3,
  b: (2 * STAGE_WIDTH) / 3,
});
/** Facing: `a` looks right (+x), `b` is mirrored and looks left. */
export const FACING: Readonly<Record<Side, 1 | -1>> = Object.freeze({ a: 1, b: -1 });

/** Theme color roles; the Pixi layer maps them through `RendererTheme` (ADR-0009 D7). */
export type ColorRole = 'hp' | 'hpLost' | 'crit' | 'text';

export interface FighterScene {
  readonly side: Side;
  readonly asset: string;
  /** The animation actually played (after the RF-A5 fallback). */
  readonly animation: AnimationCode;
  /** Position inside the animation's `frames`. */
  readonly step: number;
  /** Index into the manifest's `frames`. */
  readonly frame: number;
  readonly x: number;
  readonly y: number;
  readonly mirror: boolean;
  readonly scale: number;
}

export interface HudFighterScene {
  readonly hp: number;
  /** HP lost recently, shown as a trail behind the bar; always ≥ `hp`. */
  readonly trail: number;
  readonly maxHp: number;
}

/** Values only: the layout is `layoutHud`, in CSS px of the container (plan-a1 decision 4). */
export interface HudScene {
  readonly a: HudFighterScene;
  readonly b: HudFighterScene;
}

export interface CameraScene {
  readonly zoom: number;
  readonly centerX: number;
  readonly centerY: number;
  readonly offsetX: number;
  readonly offsetY: number;
}

export type BannerCode = 'fight' | 'ko' | 'time';

export interface BannerScene {
  readonly code: BannerCode;
  readonly alpha: number;
  readonly scale: number;
}

export interface ParticleEffect {
  readonly kind: 'particle';
  readonly x: number;
  readonly y: number;
  readonly size: number;
  readonly alpha: number;
  readonly color: ColorRole;
}

export interface DamageNumberEffect {
  readonly kind: 'damage';
  readonly cue: number;
  readonly value: number;
  readonly crit: boolean;
  readonly x: number;
  readonly y: number;
  readonly alpha: number;
  readonly scale: number;
  readonly color: ColorRole;
}

/** Tint of the struck sprite (not a full-screen flash). */
export interface FlashEffect {
  readonly kind: 'flash';
  readonly cue: number;
  readonly side: Side;
  readonly alpha: number;
}

export type EffectScene = ParticleEffect | DamageNumberEffect | FlashEffect;

export type Phase = 'intro' | 'fight' | 'ko' | 'time_up' | 'win' | 'draw' | 'finished';

export interface Scene {
  readonly t: number;
  readonly phase: Phase;
  readonly winner: Side | null;
  /** How the fight ended, from the closing cue (`win` or `draw`) on; null before (005 RF7). */
  readonly ending: Ending | null;
  readonly fighters: Readonly<Record<Side, FighterScene>>;
  readonly hud: HudScene;
  readonly camera: CameraScene;
  readonly banners: readonly BannerScene[];
  readonly effects: readonly EffectScene[];
}

export interface SceneOptions {
  readonly reducedMotion: boolean;
}
