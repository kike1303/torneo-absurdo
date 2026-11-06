// The fighters' sideways movement in the 3D arena (018), as choreography only: the engine is 1D
// and does not change, so neither does the result. Two pure functions of the scene, the same for
// every viewer at the same `t`:
// - the fight axis turns slowly around the center of the stage, so the two seem to circle each
//   other (the camera does not follow it, which is what makes it show);
// - a dodge steps out of the axis and back.
import type { DodgeCut, FighterScene, Scene } from '../scene/types.ts';
import type { Side } from '../timeline/types.ts';

/** How far the axis turns each way, in radians (~17°): more would put one fighter behind the other. */
export const CIRCLE_ANGLE = 0.3;
export const CIRCLE_PERIOD_MS = 20_000;
/** How far a dodge steps out of the axis, in meters. */
export const SIDESTEP_M = 0.28;
/**
 * How long a fighter takes back to the axis when its dodge is cut short: the counterattack
 * usually starts halfway through the dodge, and snapping back would be a jump of ~0.27 m.
 */
export const SETTLE_MS = 250;

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

/** The step of a dodge of `side` that started at `start`, `elapsed` ms into its `total`. */
function stepAt(side: Side, start: number, elapsed: number, total: number): number {
  const p = Math.min(1, Math.max(0, elapsed / total));
  return stepSide(side, start) * SIDESTEP_M * Math.sin(Math.PI * p);
}

const smoothstep = (x: number): number => {
  const c = Math.min(1, Math.max(0, x));
  return c * c * (3 - 2 * c);
};

/** What is left of a cut dodge's step at `t`: from where it was cut back to 0 over `SETTLE_MS`. */
function settle(side: Side, cut: DodgeCut, t: number): number {
  const span = Math.min(SETTLE_MS, cut.start + cut.total - cut.at);
  if (span <= 0) return 0;
  return (
    stepAt(side, cut.start, cut.at - cut.start, cut.total) * (1 - smoothstep((t - cut.at) / span))
  );
}

/**
 * How far `fighter` stands off the axis, in meters (positive towards the camera at angle 0): out
 * and back over a dodge, with an ease, then 0. A dodge cut short eases back instead of jumping;
 * the two add up, so a new dodge during that ease does not jump either.
 */
export function sidestep(
  fighter: Pick<FighterScene, 'side' | 'animation' | 'elapsed' | 'total' | 'dodgeCut'>,
  scene: Pick<Scene, 't' | 'reducedMotion'>,
): number {
  if (scene.reducedMotion) return 0;
  const cut = fighter.dodgeCut === null ? 0 : settle(fighter.side, fighter.dodgeCut, scene.t);
  if (fighter.animation !== 'dodge' || fighter.total <= 0) return cut;
  return cut + stepAt(fighter.side, scene.t - fighter.elapsed, fighter.elapsed, fighter.total);
}

/**
 * How far off the axis a logical `x` between the fighters stands: from one fighter's sidestep to
 * the other's, so a damage number stays on its fighter and an impact between them.
 */
export function depthAt(x: number, scene: Pick<Scene, 't' | 'reducedMotion' | 'fighters'>): number {
  const { a, b } = scene.fighters;
  const da = sidestep(a, scene);
  const db = sidestep(b, scene);
  const span = b.x - a.x;
  if (da === db || span === 0) return (da + db) / 2;
  return da + (db - da) * Math.min(1, Math.max(0, (x - a.x) / span));
}
