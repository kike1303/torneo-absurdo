// Screen shake as a function of (seed, τ) (RF-A12): zero outside [0, SHAKE_MS].
import { mulberry32 } from './prng.ts';

export const SHAKE_MS = 250;

export type ShakeKind = 'hit' | 'crit' | 'special' | 'ko';

/** Peak amplitude in logical px: light on `hit`, strong on `crit`, `special` and KO. */
export const SHAKE_AMPLITUDE: Readonly<Record<ShakeKind, number>> = Object.freeze({
  hit: 3,
  crit: 7,
  special: 7,
  ko: 7,
});

export interface Offset {
  readonly x: number;
  readonly y: number;
}

export const NO_OFFSET: Offset = Object.freeze({ x: 0, y: 0 });

/** Offset of the camera `tau` ms after the shake starts. */
export function shakeOffset(seed: number, kind: ShakeKind, tau: number): Offset {
  if (!(tau >= 0 && tau < SHAKE_MS)) return NO_OFFSET;
  const rng = mulberry32(seed);
  const fx = 18 + rng() * 8;
  const fy = 18 + rng() * 8;
  const px = rng() * 2 * Math.PI;
  const py = rng() * 2 * Math.PI;
  const decay = (1 - tau / SHAKE_MS) ** 2;
  const amplitude = SHAKE_AMPLITUDE[kind] * decay;
  const s = tau / 1000;
  return {
    x: amplitude * Math.sin(2 * Math.PI * fx * s + px),
    y: amplitude * Math.sin(2 * Math.PI * fy * s + py),
  };
}
