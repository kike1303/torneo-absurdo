import { describe, expect, it } from 'vitest';
import { FIXED_CAMERA } from '../src/scene/camera.ts';
import { CIRCLE_ANGLE } from '../src/model/circling.ts';
import { FRONT_SHOT } from '../src/model/shot.ts';
import { GROUND_Y, STAGE_WIDTH } from '../src/scene/types.ts';
import { arenaPoint, createCamera, placeCamera } from '../src/three/world.ts';

// 013 review: the punch zoom centers between the fighters; the camera must not jump when the zoom
// leaves or reaches 1, as the 2D transform does not.
describe('placeCamera', () => {
  it('is continuous when the zoom reaches 1, with the zoom centered off the middle', () => {
    const at = (zoom: number) => {
      const camera = createCamera();
      placeCamera(camera, 16 / 9, { ...FIXED_CAMERA, zoom, centerX: 395, centerY: 260 });
      return camera.position.clone();
    };
    const one = at(1);
    const near = at(1 + 1e-6);
    expect(near.distanceTo(one)).toBeLessThan(1e-4);
    // A real zoom moves towards the fighters' midpoint and closer.
    const zoomed = at(1.2);
    expect(zoomed.x).toBeGreaterThan(one.x);
    expect(zoomed.z).toBeLessThan(one.z);
  });

  it('steps back with the turn of the fight axis, smoothly through the front (018)', () => {
    const distance = (angle: number) => {
      const camera = createCamera();
      placeCamera(camera, 16 / 9, FIXED_CAMERA, FRONT_SHOT, angle);
      const target = arenaPoint(STAGE_WIDTH / 2, GROUND_Y, angle);
      return Math.hypot(camera.position.x - target.x, camera.position.z - target.z);
    };
    const front = distance(0);
    expect(distance(CIRCLE_ANGLE) / front).toBeCloseTo(1.15);
    expect(distance(-CIRCLE_ANGLE)).toBeCloseTo(distance(CIRCLE_ANGLE));
    // No kink at 0 (review of #102): the first step away barely moves it.
    const step = CIRCLE_ANGLE / 100;
    expect((distance(step) - front) / (distance(CIRCLE_ANGLE) - front)).toBeLessThan(0.001);
  });
});
