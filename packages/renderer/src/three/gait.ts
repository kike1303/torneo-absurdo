// Reads the root motion of the walk clips and takes it out of them (TA-74). A walk of the botarga
// carries its own advance in the hips; the arena already moves the fighter, so left in the clip it
// would count twice and snap back when the clip ends. What is removed is returned as `Gaits`, and
// `poseFor` uses it to show the frame that matches the ground covered.
import { Matrix3, Vector3, type AnimationClip, type Object3D } from 'three';
import { rootMotion, type Gaits, type RootMotion, type WalkClip } from '../model/gait.ts';

const WALK_CLIPS: readonly WalkClip[] = ['walk_forward', 'walk_back'];

/** The bone the skeleton hangs from: the first one whose parent is not a bone. */
function rootBone(scene: Object3D): Object3D | null {
  let found: Object3D | null = null;
  scene.traverse((node) => {
    if (found !== null || !(node as { isBone?: boolean }).isBone) return;
    if (!(node.parent as { isBone?: boolean } | null)?.isBone) found = node;
  });
  return found;
}

/**
 * Makes `clip` walk in place along the model's facing (+z, as glTF) and returns the advance it
 * had. Null, and the clip untouched, when it has no root track to read or it walks in place.
 */
function ground(clip: AnimationClip, root: Object3D): RootMotion | null {
  const track = clip.tracks.find((t) => t.name === `${root.name}.position`);
  const parent = root.parent;
  if (track === undefined || parent === null) return null;
  const { times, values } = track;
  // One vec3 per keyframe: a cubic spline track (three per keyframe) is left as it is.
  if (values.length !== times.length * 3) return null;
  const toWorld = new Matrix3().setFromMatrix4(parent.matrixWorld);
  const toLocal = toWorld.clone().invert();
  const start = new Vector3().fromArray(values, 0);
  const delta = new Vector3();
  const advance: number[] = [];
  for (let i = 0; i < times.length; i++) {
    advance.push(
      delta
        .fromArray(values, i * 3)
        .sub(start)
        .applyMatrix3(toWorld).z,
    );
  }
  const motion = rootMotion(Array.from(times), advance);
  if (motion === null) return null;
  for (let i = 0; i < times.length; i++) {
    delta.set(0, 0, advance[i] as number).applyMatrix3(toLocal);
    values[i * 3] = (values[i * 3] as number) - delta.x;
    values[i * 3 + 1] = (values[i * 3 + 1] as number) - delta.y;
    values[i * 3 + 2] = (values[i * 3 + 2] as number) - delta.z;
  }
  return motion;
}

/** Grounds the walk clips of a loaded GLB, once, and returns their root motion. */
export function groundWalks(scene: Object3D, clips: ReadonlyMap<string, AnimationClip>): Gaits {
  const root = rootBone(scene);
  const gaits: Partial<Record<WalkClip, RootMotion>> = {};
  if (root === null) return gaits;
  scene.updateMatrixWorld(true);
  for (const name of WALK_CLIPS) {
    const clip = clips.get(name);
    const motion = clip === undefined ? null : ground(clip, root);
    if (motion !== null) gaits[name] = motion;
  }
  return gaits;
}
