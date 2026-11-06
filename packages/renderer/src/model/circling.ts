// The fighters' sideways movement in the 3D arena (018), as choreography only: the engine is 1D
// and does not change, so neither does the result. Two pure functions of the scene, the same for
// every viewer at the same `t`:
// - the fight axis turns slowly around the center of the stage, so the two seem to circle each
//   other (the camera does not follow it, which is what makes it show);
// - a dodge steps out of the axis and back.
import type { FighterScene, Scene } from '../scene/types.ts';
import type { Side } from '../timeline/types.ts';

/** How far the axis turns each way, in radians (~17°): more would put one fighter behind the other. */
export const CIRCLE_ANGLE = 0.3;
export const CIRCLE_PERIOD_MS = 20_000;
/** How far a dodge steps out of the axis, in meters. */
export const SIDESTEP_M = 0.28;

/** The angle of the fight axis at `scene.t`: 0 is the axis of the 2D stage (left to right). */
export function arenaAngle(scene: Pick<Scene, 't' | 'reducedMotion'>): number {
  if (scene.reducedMotion) return 0;
  return CIRCLE_ANGLE * Math.sin((2 * Math.PI * Math.max(0, scene.t)) / CIRCLE_PERIOD_MS);
}

/** FNV-1a over the side and the start of the dodge: which way it steps, the same on every client. */
function stepSide(side: Side, start: number): 1 | -1 {
  let h = 2166136261;
  for (const v of [side === 'a' ? 1 : 2, Math.round(start)]) {
    for (let shift = 0; shift < 32; shift += 8) {
      h ^= (v >>> shift) & 255;
      h = Math.imul(h, 16777619) >>> 0;
    }
  }
  return h % 2 === 0 ? 1 : -1;
}

/**
 * How far `fighter` stands off the axis, in meters (positive towards the camera at angle 0): out
 * and back over a dodge, with an ease, and 0 otherwise.
 */
export function sidestep(
  fighter: Pick<FighterScene, 'side' | 'animation' | 'elapsed' | 'total'>,
  scene: Pick<Scene, 't' | 'reducedMotion'>,
): number {
  if (scene.reducedMotion || fighter.animation !== 'dodge' || fighter.total <= 0) return 0;
  const p = Math.min(1, Math.max(0, fighter.elapsed / fighter.total));
  return stepSide(fighter.side, scene.t - fighter.elapsed) * SIDESTEP_M * Math.sin(Math.PI * p);
}
