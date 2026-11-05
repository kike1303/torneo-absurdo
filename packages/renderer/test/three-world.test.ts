import { describe, expect, it } from 'vitest';
import { FIXED_CAMERA } from '../src/scene/camera.ts';
import { createCamera, placeCamera } from '../src/three/world.ts';

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
});
