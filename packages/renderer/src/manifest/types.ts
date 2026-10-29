// Sprite manifest, format v1 (spec "Diseño → Formato de assets v1", RF-A4 and RF-A5).
import { TIMELINE_LIMITS } from '../timeline/types.ts';

export const MANIFEST_FORMAT = 'torneo.sprites';
export const MANIFEST_FORMAT_VERSION = 1;

/** Animations every manifest must declare (RF-A5). */
export const REQUIRED_ANIMATIONS = [
  'idle',
  'attack_light',
  'attack_heavy',
  'special',
  'hit',
  'block',
  'dodge',
  'ko',
  'win',
] as const;

/** Animations a manifest may omit; the format fixes their fallback (RF-A5). */
export const OPTIONAL_ANIMATIONS = ['intro', 'walk', 'knockdown'] as const;

export type RequiredAnimation = (typeof REQUIRED_ANIMATIONS)[number];
export type OptionalAnimation = (typeof OPTIONAL_ANIMATIONS)[number];
export type AnimationCode = RequiredAnimation | OptionalAnimation;

export const ANIMATION_FALLBACK: Readonly<Record<OptionalAnimation, RequiredAnimation>> =
  Object.freeze({ intro: 'idle', walk: 'idle', knockdown: 'hit' });

/** Animations that declare `impact_frame` (and only these). */
export const ATTACK_ANIMATIONS = ['attack_light', 'attack_heavy', 'special'] as const;
export type AttackAnimation = (typeof ATTACK_ANIMATIONS)[number];

/** Animations that must loop, and animations that must not. */
export const LOOPING_ANIMATIONS: readonly AnimationCode[] = ['idle', 'walk'];
export const NON_LOOPING_ANIMATIONS: readonly AnimationCode[] = ['ko'];

/**
 * The choreography guarantee of 002: with impacts at least `strikeGapMs` apart and windups of at
 * most `windupMaxMs`, a fighter's next windup starts at least this long after an impact it
 * received, so the reaction is always seen (007 RF-A1).
 */
export const REACTION_WINDOW_MS = 200;

export const LIMITS = Object.freeze({
  idPattern: /^[a-z][a-z0-9_]{0,47}$/,
  sheetFilePattern: /^sheet-[0-9]+\.png$/,
  sheetsMin: 1,
  sheetsMax: 4,
  sheetSideMax: 2048,
  framesMin: 1,
  framesMax: 1024,
  animationFramesMin: 1,
  animationFramesMax: 64,
  frameMsMin: 16,
  frameMsMax: 2000,
  nonLoopingTotalMaxMs: 3000,
  /** Derived, not chosen: the gap between impacts minus the reaction window (007 RF-A1). */
  windupMaxMs: TIMELINE_LIMITS.strikeGapMs - REACTION_WINDOW_MS,
  scaleMin: 0.25,
  scaleMax: 4,
  maxErrors: 100,
});

export interface SpriteSheet {
  readonly file: string;
  readonly width: number;
  readonly height: number;
}

export interface SpriteFrame {
  readonly sheet: number;
  readonly x: number;
  readonly y: number;
  readonly w: number;
  readonly h: number;
  /** The feet, in frame coordinates. */
  readonly pivot_x: number;
  readonly pivot_y: number;
}

export interface SpriteAnimation {
  /** Indices into `frames`. */
  readonly frames: readonly number[];
  readonly frame_ms: readonly number[];
  readonly loop: boolean;
  /** Index into this animation's `frames` (attacks only). */
  readonly impact_frame?: number;
}

export type SpriteAnimations = Readonly<Record<RequiredAnimation, SpriteAnimation>> &
  Readonly<Partial<Record<OptionalAnimation, SpriteAnimation>>>;

export interface SpriteManifest {
  readonly format: typeof MANIFEST_FORMAT;
  readonly format_version: typeof MANIFEST_FORMAT_VERSION;
  readonly id: string;
  readonly pixel_art: boolean;
  readonly scale?: number;
  readonly sheets: readonly SpriteSheet[];
  readonly frames: readonly SpriteFrame[];
  readonly animations: SpriteAnimations;
}

/** A validation error: a code and a JSON Pointer, never text (ADR-0007). */
export interface ManifestError {
  readonly code: ManifestErrorCode;
  readonly path: string;
  readonly data?: Readonly<Record<string, string | number>>;
}

export type ManifestErrorCode = `manifest.${string}`;
