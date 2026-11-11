// Where the fighters stand in the 3D arena (TA-74). The 2D stage keeps its marks a third of the
// width apart, so a fighter crosses 149 px to strike: 1.07 m for a botarga 1 m tall. Its walk is a
// guard step of 0.2 m, so it skated. In 3D the two stand closer, three of those steps away from
// contact; a strike still connects `CONTACT_PX` apart. Presentation only, as a pure rewrite of the
// scene: the choreography, the engine and the result do not change, and every viewer sees the same
// at the same `t`.
import {
  CONTACT_PX,
  MARK_X,
  type EffectScene,
  type FighterScene,
  type Scene,
} from '../scene/types.ts';
import type { Side } from '../timeline/types.ts';

/** Horizontal scale of the 3D arena: a strike connects at ~0.46 m (`CONTACT_PX`). */
export const STAGE_METERS_PER_PX = 1 / 140;
/**
 * How far a fighter goes in to strike and back out, in meters: three plays of the botarga's guard
 * step back (its hips advance 0.197 m in `walk_back` and 0.180 m in `walk_forward`, measured on
 * the GLB). The marks end up ~1.05 m apart (owner's pick, 2026-11-10: closer read as crowded,
 * the 1.5 m of the 2D stage as a dash).
 */
export const STEP_M = 0.59;

/** What a fighter crosses on the 2D stage from its mark to contact with a rival on its own. */
const TRIP_PX = MARK_X.b - MARK_X.a - CONTACT_PX;
/** How much of its 2D trip a fighter covers in 3D. */
export const STEP_SCALE = STEP_M / (TRIP_PX * STAGE_METERS_PER_PX);
const CENTER_X = (MARK_X.a + MARK_X.b) / 2;
const HALF_GAP_PX = (CONTACT_PX + TRIP_PX * STEP_SCALE) / 2;

/** The marks of the 3D arena, in the logical px of the stage: contact distance plus `STEP_M`. */
export const STANCE_MARK_X: Readonly<Record<Side, number>> = Object.freeze({
  a: CENTER_X - HALF_GAP_PX,
  b: CENTER_X + HALF_GAP_PX,
});

/** Where a fighter of `side` at the logical `x` of the 2D stage stands in the 3D arena. */
export function stanceX(side: Side, x: number): number {
  return STANCE_MARK_X[side] + (x - MARK_X[side]) * STEP_SCALE;
}

/**
 * Meters between a fighter of a `stanceScene` and its mark: what is left of a return, or what it
 * has stepped in.
 */
export function metersFromMark(fighter: Pick<FighterScene, 'side' | 'x'>): number {
  return Math.abs(fighter.x - STANCE_MARK_X[fighter.side]) * STAGE_METERS_PER_PX;
}

/** A point of the 2D stage that is not a fighter: it keeps its place relative to the two. */
function pointX(x: number, from: Record<Side, number>, to: Record<Side, number>): number {
  if (x <= from.a) return to.a + (x - from.a);
  if (x >= from.b) return to.b + (x - from.b);
  return to.a + ((x - from.a) / (from.b - from.a)) * (to.b - to.a);
}

/**
 * `scene` as the 3D arena stages it: the fighters at their 3D stance, and the effects and the
 * zoom center where they were relative to them (a damage number stays over its fighter, an impact
 * between the two).
 */
export function stanceScene(scene: Scene): Scene {
  const { a, b } = scene.fighters;
  const from = { a: a.x, b: b.x };
  const to = { a: stanceX('a', a.x), b: stanceX('b', b.x) };
  const effects = scene.effects.map((effect): EffectScene =>
    effect.kind === 'flash' ? effect : { ...effect, x: pointX(effect.x, from, to) },
  );
  // At zoom 1 the center is the middle of the stage, not a point between the fighters.
  const camera =
    scene.camera.zoom === 1
      ? scene.camera
      : { ...scene.camera, centerX: pointX(scene.camera.centerX, from, to) };
  return {
    ...scene,
    fighters: { a: { ...a, x: to.a }, b: { ...b, x: to.b } },
    camera,
    effects,
  };
}
