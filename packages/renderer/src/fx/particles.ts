// Impact particles as a closed-form function of (seed, τ) (RF-A12, RF-A9): no incremental
// simulation, so `seek(t)` shows the same as playing up to `t`.
import { mulberry32 } from './prng.ts';

/** Hard cap of live particles in a scene (RF-A12). */
export const MAX_PARTICLES = 64;

export type ParticleKind = 'hit' | 'crit' | 'special';

export const PARTICLE_COUNT: Readonly<Record<ParticleKind, number>> = Object.freeze({
  hit: 10,
  crit: 18,
  special: 24,
});

/** Longest particle life; shorter than the 600 ms between impacts. */
export const PARTICLE_LIFE_MS = 450;
const GRAVITY = 900; // logical px / s²

export interface ParticleState {
  readonly x: number;
  readonly y: number;
  readonly size: number;
  readonly alpha: number;
}

/** Live particles of one burst `tau` ms after the impact at (`ox`, `oy`). */
export function particlesAt(
  seed: number,
  kind: ParticleKind,
  tau: number,
  ox: number,
  oy: number,
  direction: 1 | -1,
): ParticleState[] {
  if (!(tau >= 0 && tau < PARTICLE_LIFE_MS)) return [];
  const rng = mulberry32(seed);
  const s = tau / 1000;
  const out: ParticleState[] = [];
  const count = PARTICLE_COUNT[kind];
  for (let i = 0; i < count; i++) {
    // Always draw the same number of values per particle, so particle i never depends on τ.
    const angle = (rng() * 0.8 - 0.55) * Math.PI;
    const speed = 140 + rng() * 180;
    const size = 2 + Math.floor(rng() * 3);
    const life = PARTICLE_LIFE_MS * (0.6 + rng() * 0.4);
    if (tau >= life) continue;
    const vx = Math.cos(angle) * speed * direction;
    const vy = Math.sin(angle) * speed;
    out.push({
      x: ox + vx * s,
      y: oy + vy * s + 0.5 * GRAVITY * s * s,
      size,
      alpha: 1 - tau / life,
    });
  }
  return out;
}
