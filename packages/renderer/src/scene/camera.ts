// Fixed camera (RF-A15) with screen shake and punch zoom (RF-A12). Reduced motion: neither.
import type { Offset } from '../fx/shake.ts';
import { SHAKE_AMPLITUDE } from '../fx/shake.ts';
import { GROUND_Y, STAGE_HEIGHT, STAGE_WIDTH, type CameraScene } from './types.ts';

/** Zoom center height: the fighters' chests. */
const ZOOM_CENTER_HEIGHT = 60;
const MAX_SHAKE = SHAKE_AMPLITUDE.ko;

export const FIXED_CAMERA: CameraScene = Object.freeze({
  zoom: 1,
  centerX: STAGE_WIDTH / 2,
  centerY: STAGE_HEIGHT / 2,
  offsetX: 0,
  offsetY: 0,
});

function clamp(v: number, limit: number): number {
  return Math.max(-limit, Math.min(limit, v));
}

export function cameraFrom(
  zoom: number,
  shake: Offset,
  fightersX: { a: number; b: number },
): CameraScene {
  if (zoom === 1 && shake.x === 0 && shake.y === 0) return { ...FIXED_CAMERA };
  return {
    zoom,
    centerX: zoom === 1 ? STAGE_WIDTH / 2 : (fightersX.a + fightersX.b) / 2,
    centerY: zoom === 1 ? STAGE_HEIGHT / 2 : GROUND_Y - ZOOM_CENTER_HEIGHT,
    offsetX: clamp(shake.x, MAX_SHAKE),
    offsetY: clamp(shake.y, MAX_SHAKE),
  };
}
