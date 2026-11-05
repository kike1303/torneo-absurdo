// Where the 3D camera looks from (014), as a pure function of the scene: every viewer gets the
// same shot at the same `t`. Angles turn around the vertical axis through the focus; 0 is the
// front view, and a positive yaw moves the camera towards +x (the side `a` faces).
import { FACING, STAGE_WIDTH, type FighterScene, type Scene } from '../scene/types.ts';
import { SIDES, type Side } from '../timeline/types.ts';

export interface Shot {
  readonly yaw: number;
  /** Multiple of the distance that frames both fighters. */
  readonly distance: number;
  /** Logical x the camera looks at, or null for the frame of `Scene.camera` (center and zoom). */
  readonly focusX: number | null;
}

/**
 * The exchange sways slowly around the front view, the same to each side: a fixed 3/4 view would
 * put one fighter in front of the other, back to the camera.
 */
const SWAY_YAW = 0.3;
const SWAY_PERIOD_MS = 14_000;
/** The intro pushes in from afar, turning a little. */
const INTRO_YAW = -0.35;
const INTRO_DISTANCE = 1.45;
const INTRO_PUSH_MS = 2600;
/** A special cuts to the attacker, a little to its front: more would put the rival in between. */
const SPECIAL_YAW = 0.4;
const SPECIAL_DISTANCE = 0.62;
/** How far towards the opponent the close-up looks, in logical px. */
const SPECIAL_LEAD_PX = 24;
/** KO and win orbit slowly around one fighter. */
const ORBIT_RAD_PER_MS = 0.00018;
const ORBIT_DISTANCE = 0.72;
const ORBIT_EASE_MS = 900;

export const FRONT_SHOT: Shot = Object.freeze({ yaw: 0, distance: 1, focusX: null });

const smoothstep = (x: number): number => {
  const c = Math.min(1, Math.max(0, x));
  return c * c * (3 - 2 * c);
};

function exchangeYaw(t: number): number {
  return SWAY_YAW * Math.sin((2 * Math.PI * t) / SWAY_PERIOD_MS);
}

function attacking(scene: Scene): FighterScene | null {
  for (const side of SIDES) {
    const f = scene.fighters[side];
    if (f.animation === 'special' && f.elapsed < f.total) return f;
  }
  return null;
}

/** From the exchange to an orbit around `fighter`, eased over its first `ORBIT_EASE_MS`. */
function orbit(scene: Scene, fighter: FighterScene): Shot {
  const ease = smoothstep(fighter.elapsed / ORBIT_EASE_MS);
  const from = exchangeYaw(scene.t - fighter.elapsed);
  const middle = (scene.fighters.a.x + scene.fighters.b.x) / 2;
  return {
    yaw: from + fighter.elapsed * ORBIT_RAD_PER_MS,
    distance: 1 + (ORBIT_DISTANCE - 1) * ease,
    focusX: middle + (fighter.x - middle) * ease,
  };
}

function sideWith(scene: Scene, animation: FighterScene['animation']): Side | null {
  return SIDES.find((side) => scene.fighters[side].animation === animation) ?? null;
}

export function shotFor(scene: Scene): Shot {
  if (scene.reducedMotion) return FRONT_SHOT;
  if (scene.phase === 'intro') {
    const push = 1 - smoothstep(scene.t / INTRO_PUSH_MS);
    return {
      yaw: exchangeYaw(scene.t) + INTRO_YAW * push,
      distance: 1 + (INTRO_DISTANCE - 1) * push,
      focusX: null,
    };
  }
  if (scene.phase === 'win' || scene.phase === 'finished') {
    const winner = scene.winner ?? sideWith(scene, 'win');
    if (winner !== null) return orbit(scene, scene.fighters[winner]);
  }
  const fallen = sideWith(scene, 'ko');
  if (fallen !== null) return orbit(scene, scene.fighters[fallen]);
  const attacker = attacking(scene);
  if (attacker !== null) {
    const facing = FACING[attacker.side];
    return {
      yaw: facing * SPECIAL_YAW,
      distance: SPECIAL_DISTANCE,
      focusX: Math.min(STAGE_WIDTH, Math.max(0, attacker.x + facing * SPECIAL_LEAD_PX)),
    };
  }
  return { yaw: exchangeYaw(scene.t), distance: 1, focusX: null };
}
