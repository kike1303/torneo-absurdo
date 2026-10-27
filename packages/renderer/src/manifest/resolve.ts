// Animation fallback and timing helpers (RF-A5).
import {
  ANIMATION_FALLBACK,
  type AnimationCode,
  type OptionalAnimation,
  type SpriteAnimation,
  type SpriteManifest,
} from './types.ts';

export interface ResolvedAnimation {
  /** The animation actually played (the fallback when the optional one is missing). */
  readonly code: AnimationCode;
  readonly animation: SpriteAnimation;
}

function isOptional(code: AnimationCode): code is OptionalAnimation {
  return Object.hasOwn(ANIMATION_FALLBACK, code);
}

export function resolveAnimation(manifest: SpriteManifest, code: AnimationCode): ResolvedAnimation {
  const own = manifest.animations[code];
  if (own !== undefined) return { code, animation: own };
  if (isOptional(code)) {
    const fallback = ANIMATION_FALLBACK[code];
    return { code: fallback, animation: manifest.animations[fallback] };
  }
  // Unreachable for a validated manifest: every required animation is present.
  throw new Error('manifest.missing_animation');
}

/** Sum of `frame_ms` before the impact frame (0 for animations without one). */
export function windupMs(animation: SpriteAnimation): number {
  const impact = animation.impact_frame;
  if (impact === undefined) return 0;
  let sum = 0;
  for (let i = 0; i < impact; i++) sum += animation.frame_ms[i] ?? 0;
  return sum;
}

/** Sum of every `frame_ms` of the animation (one cycle when it loops). */
export function totalMs(animation: SpriteAnimation): number {
  let sum = 0;
  for (const ms of animation.frame_ms) sum += ms;
  return sum;
}
