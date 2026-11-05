// From the logical stage of `Scene` (640 × 360 px, RF-A15) to the 3D world in meters (013). The
// botarga is 1 m tall, stands on y = 0 and the fight runs along x.
import { PerspectiveCamera } from 'three';
import { GROUND_Y, STAGE_WIDTH, type CameraScene } from '../scene/types.ts';

/** Horizontal scale: the marks end up ~1.5 m apart and a strike connects at ~0.46 m. */
export const METERS_PER_PX = 1 / 140;
/** Vertical scale, so the impact height (56 px) is the chest and the damage numbers the head. */
export const HEIGHT_PER_PX = 1 / 120;
/**
 * How much each fighter turns from its profile towards the camera, so the face reads (a pure
 * profile hides it).
 */
export const TURN_TO_CAMERA = 0.45;

export const worldX = (x: number): number => (x - STAGE_WIDTH / 2) * METERS_PER_PX;
export const worldY = (y: number): number => (GROUND_Y - y) * HEIGHT_PER_PX;

const FOV_DEG = 30;
/** What the fixed camera shows at the fighters' depth. */
const VIEW_HEIGHT = 2;
const VIEW_WIDTH = 3.6;
const TARGET_HEIGHT = 0.6;
/** How far above the target the camera sits, for a slight top-down look. */
const CAMERA_LIFT = 0.35;

export function createCamera(): PerspectiveCamera {
  return new PerspectiveCamera(FOV_DEG, 16 / 9, 0.05, 50);
}

/** Frames both fighters for `aspect`, then applies the zoom and the shake of `scene`. */
export function placeCamera(camera: PerspectiveCamera, aspect: number, scene: CameraScene): void {
  camera.aspect = aspect;
  const half = Math.tan(((FOV_DEG / 2) * Math.PI) / 180);
  const base = Math.max(VIEW_HEIGHT / 2 / half, VIEW_WIDTH / 2 / (half * Math.max(0.1, aspect)));
  const distance = base / Math.max(0.1, scene.zoom);
  // What the center of the screen shows under the 2D transform (center + zoom × (p − center)):
  // continuous in the zoom, so the camera never jumps when a punch zoom starts or ends.
  const zoom = Math.max(0.1, scene.zoom);
  const focus = scene.centerX + (STAGE_WIDTH / 2 - scene.centerX) / zoom;
  const x = worldX(focus) + scene.offsetX * METERS_PER_PX;
  const y = TARGET_HEIGHT - scene.offsetY * HEIGHT_PER_PX;
  camera.position.set(x, y + CAMERA_LIFT, distance);
  camera.lookAt(x, y, 0);
  camera.updateProjectionMatrix();
}
