// From the logical stage of `Scene` (640 × 360 px, RF-A15) to the 3D world in meters (013). The
// botarga is 1 m tall, stands on y = 0 and the fight runs along x.
import { PerspectiveCamera } from 'three';
import { CIRCLE_ANGLE } from '../model/circling.ts';
import { FRONT_SHOT, type Shot } from '../model/shot.ts';
import { STAGE_METERS_PER_PX } from '../model/stance.ts';
import { GROUND_Y, STAGE_WIDTH, type CameraScene } from '../scene/types.ts';
import type { Side } from '../timeline/types.ts';

/**
 * Horizontal scale: a strike connects at ~0.46 m. The fighters stand where `stanceScene` puts them
 * (TA-74), ~1.05 m apart at their marks.
 */
export const METERS_PER_PX = STAGE_METERS_PER_PX;
/** Vertical scale, so the impact height (56 px) is the chest and the damage numbers the head. */
export const HEIGHT_PER_PX = 1 / 120;
/**
 * How much each fighter turns from its profile towards the camera, so the face reads (a pure
 * profile hides it).
 */
export const TURN_TO_CAMERA = 0.45;

/**
 * The yaw of a fighter of `side` (a glTF model faces +z): towards its rival along the fight axis
 * turned `angle` (018), then `turnToCamera` towards the camera.
 */
export function facingYaw(side: Side, angle: number, turnToCamera = TURN_TO_CAMERA): number {
  return angle + (side === 'a' ? Math.PI / 2 - turnToCamera : turnToCamera - Math.PI / 2);
}

export const worldX = (x: number): number => (x - STAGE_WIDTH / 2) * METERS_PER_PX;
export const worldY = (y: number): number => (GROUND_Y - y) * HEIGHT_PER_PX;

export interface ArenaPoint {
  readonly x: number;
  readonly y: number;
  readonly z: number;
}

/**
 * Where a logical point of the stage stands in the 3D arena (018): on the fight axis turned
 * `angle` around the center of the stage, `depth` meters off it (positive towards the camera at
 * angle 0). With angle 0 and depth 0 it is `(worldX(x), worldY(y), 0)`.
 */
export function arenaPoint(x: number, y: number, angle: number, depth = 0): ArenaPoint {
  const along = worldX(x);
  const cos = Math.cos(angle);
  const sin = Math.sin(angle);
  return { x: along * cos + depth * sin, y: worldY(y), z: depth * cos - along * sin };
}

const FOV_DEG = 30;
/** What the fixed camera shows at the fighters' depth. */
const VIEW_HEIGHT = 2;
const VIEW_WIDTH = 3.6;
const TARGET_HEIGHT = 0.6;
/** How far above the target the camera sits, for a slight top-down look. */
const CAMERA_LIFT = 0.35;
/**
 * How much farther the camera stands at the widest turn of the fight axis (018): the fighter that
 * comes towards it grows and drifts to the edge, and would be cut. It grows with the square of the
 * turn, so the dolly eases through the front instead of bouncing off it.
 */
const TURN_PULLBACK = 0.15;

export function createCamera(): PerspectiveCamera {
  return new PerspectiveCamera(FOV_DEG, 16 / 9, 0.05, 50);
}

/**
 * Frames both fighters for `aspect`, then applies the zoom and the shake of `scene`, the angle,
 * distance and focus of `shot` (014), and the turn of the fight axis (`angle`, 018), which moves
 * the focus but not the camera's own angle.
 */
export function placeCamera(
  camera: PerspectiveCamera,
  aspect: number,
  scene: CameraScene,
  shot: Shot = FRONT_SHOT,
  angle = 0,
): void {
  camera.aspect = aspect;
  const half = Math.tan(((FOV_DEG / 2) * Math.PI) / 180);
  const base = Math.max(VIEW_HEIGHT / 2 / half, VIEW_WIDTH / 2 / (half * Math.max(0.1, aspect)));
  const zoom = Math.max(0.1, scene.zoom);
  const turn = angle / CIRCLE_ANGLE;
  const distance = (base * shot.distance * (1 + TURN_PULLBACK * turn * turn)) / zoom;
  // What the center of the screen shows under the 2D transform (center + zoom × (p − center)):
  // continuous in the zoom, so the camera never jumps when a punch zoom starts or ends.
  const focus = shot.focusX ?? scene.centerX + (STAGE_WIDTH / 2 - scene.centerX) / zoom;
  const target = arenaPoint(focus, GROUND_Y, angle);
  const x = target.x + scene.offsetX * METERS_PER_PX;
  const z = target.z;
  const y = TARGET_HEIGHT - scene.offsetY * HEIGHT_PER_PX;
  camera.position.set(
    x + Math.sin(shot.yaw) * distance,
    y + CAMERA_LIFT * shot.distance,
    z + Math.cos(shot.yaw) * distance,
  );
  camera.lookAt(x, y, z);
  camera.updateProjectionMatrix();
}
