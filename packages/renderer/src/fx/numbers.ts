// Floating damage numbers (RF-A12): rise and fade within NUMBER_LIFE_MS, with a horizontal
// spread from the PRNG. With reduced motion they stay still (RF-A14).
import { mulberry32 } from './prng.ts';

export const NUMBER_LIFE_MS = 900;
const RISE_PX = 44;
const SPREAD_PX = 18;
const FADE_FROM_MS = 600;
const POP_MS = 120;

export interface NumberState {
  readonly dx: number;
  readonly dy: number;
  readonly alpha: number;
  readonly scale: number;
}

export function damageNumberAt(
  seed: number,
  tau: number,
  crit: boolean,
  reducedMotion: boolean,
): NumberState | null {
  if (!(tau >= 0 && tau < NUMBER_LIFE_MS)) return null;
  const base = crit ? 1.5 : 1;
  if (reducedMotion) return { dx: 0, dy: 0, alpha: 1, scale: base };
  const rng = mulberry32(seed);
  const dx = (rng() * 2 - 1) * SPREAD_PX;
  const p = tau / NUMBER_LIFE_MS;
  const dy = -RISE_PX * (1 - (1 - p) ** 2);
  const alpha = tau < FADE_FROM_MS ? 1 : 1 - (tau - FADE_FROM_MS) / (NUMBER_LIFE_MS - FADE_FROM_MS);
  const pop = tau < POP_MS ? 1 + 0.3 * (1 - tau / POP_MS) : 1;
  return { dx, dy, alpha, scale: base * pop };
}
