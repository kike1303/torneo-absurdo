import { AnimationClip, Bone, Group, Object3D, VectorKeyframeTrack } from 'three';
import { describe, expect, it } from 'vitest';
import { strideOf } from '../src/model/gait.ts';
import { groundWalks } from '../src/three/gait.ts';

/** A skeleton like the exported botarga: an armature scaled and turned over the hips. */
function rig(): { scene: Group; hips: Bone } {
  const scene = new Group();
  const armature = new Object3D();
  armature.scale.setScalar(0.01);
  armature.rotation.x = Math.PI / 2;
  const hips = new Bone();
  hips.name = 'Hips';
  const spine = new Bone();
  spine.name = 'Spine';
  hips.add(spine);
  armature.add(hips);
  scene.add(armature);
  return { scene, hips };
}

/** Hips keyframes whose world position is `world[i]` under the armature of `rig`. */
function hipsTrack(scene: Group, hips: Bone, world: readonly [number, number, number][]) {
  scene.updateMatrixWorld(true);
  const values = world.flatMap(([x, y, z]) => {
    const local = (hips.parent as Object3D).worldToLocal(hips.position.clone().set(x, y, z));
    return [local.x, local.y, local.z];
  });
  return new VectorKeyframeTrack(
    'Hips.position',
    world.map((_, i) => i * 0.5),
    values,
  );
}

function worldAt(scene: Group, hips: Bone, clip: AnimationClip, key: number) {
  const values = (clip.tracks[0] as VectorKeyframeTrack).values;
  hips.position.fromArray(values, key * 3);
  scene.updateMatrixWorld(true);
  return hips.getWorldPosition(hips.position.clone());
}

describe('groundWalks (TA-74)', () => {
  it('takes the advance along the facing out of the walks and returns it', () => {
    const { scene, hips } = rig();
    const forward = new AnimationClip('walk_forward', 1, [
      hipsTrack(scene, hips, [
        [0, 0.5, -0.05],
        [0.01, 0.52, 0.05],
        [0, 0.5, 0.13],
      ]),
    ]);
    const back = new AnimationClip('walk_back', 1, [
      hipsTrack(scene, hips, [
        [0, 0.5, -0.05],
        [0, 0.51, -0.15],
        [0, 0.5, -0.25],
      ]),
    ]);
    const gaits = groundWalks(
      scene,
      new Map([
        ['walk_forward', forward],
        ['walk_back', back],
      ]),
    );
    expect(gaits.walk_forward?.times).toEqual([0, 0.5, 1]);
    expect(gaits.walk_forward?.distance).toEqual([0, 0.1, 0.18].map((d) => expect.closeTo(d)));
    expect(strideOf(gaits.walk_back as NonNullable<typeof gaits.walk_back>)).toBeCloseTo(0.2);
    // In place: the hips keep their sway and their bob, and stay where the clip started.
    const mid = worldAt(scene, hips, forward, 1);
    expect(mid.x).toBeCloseTo(0.01);
    expect(mid.y).toBeCloseTo(0.52);
    expect(mid.z).toBeCloseTo(-0.05);
    expect(worldAt(scene, hips, forward, 2).z).toBeCloseTo(-0.05);
    expect(worldAt(scene, hips, back, 2).z).toBeCloseTo(-0.05);
  });

  it('leaves alone a clip in place, another clip and a GLB without the walks', () => {
    const { scene, hips } = rig();
    const still: [number, number, number][] = [
      [0, 0.5, 0],
      [0, 0.52, 0.001],
    ];
    const inPlace = new AnimationClip('walk_forward', 1, [hipsTrack(scene, hips, still)]);
    const before = Array.from((inPlace.tracks[0] as VectorKeyframeTrack).values);
    const attack = new AnimationClip('attack_light', 1, [
      hipsTrack(scene, hips, [
        [0, 0.5, 0],
        [0, 0.5, 0.3],
      ]),
    ]);
    const lunge = Array.from((attack.tracks[0] as VectorKeyframeTrack).values);
    const gaits = groundWalks(
      scene,
      new Map([
        ['walk_forward', inPlace],
        ['attack_light', attack],
      ]),
    );
    expect(gaits).toEqual({});
    expect(Array.from((inPlace.tracks[0] as VectorKeyframeTrack).values)).toEqual(before);
    expect(Array.from((attack.tracks[0] as VectorKeyframeTrack).values)).toEqual(lunge);
    expect(groundWalks(new Group(), new Map())).toEqual({});
  });
});
