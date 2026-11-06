// One botarga in the arena (013): a clone of the GLB with its own materials (tint and face), and
// the clip of `poseFor` sampled at its time. Only the fade between clips keeps state, and it is
// cosmetic: the pose itself is a function of `t`.
import {
  AnimationMixer,
  Color,
  LoopOnce,
  MeshLambertMaterial,
  Vector3,
  type AnimationAction,
  type Material,
  type Mesh,
  type MeshStandardMaterial,
  type Object3D,
  type Texture,
} from 'three';
import { clone as cloneSkinned } from 'three/addons/utils/SkeletonUtils.js';
import type { ClipName, Pose } from '../model/pose.ts';
import type { Side } from '../timeline/types.ts';
import type { Botarga } from './load.ts';
import type { BotargaTint } from './types.ts';
import { GROUND_Y, type FighterScene } from '../scene/types.ts';
import { TURN_TO_CAMERA, arenaPoint } from './world.ts';

/** Length of the cross-fade between two clips. */
export const FADE_MS = 200;
/** Strength of the hit flash, as emissive light over the material. */
const FLASH_STRENGTH = 0.6;
/** Keeps a held clip off its exact end, where `LoopOnce` would stop sampling it. */
const END_EPSILON = 1e-4;

/** PBR as exported, or diffuse only (`lite`): a far smaller shader, for software WebGL. */
type BotargaMaterial = MeshStandardMaterial | MeshLambertMaterial;

const SHORTS = 'Botarga_Short';
const FACE = 'Botarga_Cara';

interface FaceRect {
  readonly u0: number;
  readonly v0: number;
  readonly size: number;
}

function parseFaceRect(value: unknown): FaceRect | null {
  try {
    const rect = (typeof value === 'string' ? JSON.parse(value) : value) as Partial<FaceRect>;
    const { u0, v0, size } = rect;
    if (typeof u0 === 'number' && typeof v0 === 'number' && typeof size === 'number')
      return { u0, v0, size };
  } catch {
    // Without a valid rect the face slot stays off: the felt shows instead.
  }
  return null;
}

/**
 * Mixes a second texture inside the face rect of the atlas, by its alpha. The rect is in Blender UV
 * space (v up) and glTF flips v, hence `1 − v`.
 */
function addFaceSlot(material: BotargaMaterial, rect: FaceRect, face: { value: Texture | null }) {
  const uniforms = { faceMap: face, faceRect: { value: new Vector3(rect.u0, rect.v0, rect.size) } };
  material.onBeforeCompile = (shader) => {
    Object.assign(shader.uniforms, uniforms);
    shader.fragmentShader =
      'uniform sampler2D faceMap;\nuniform vec3 faceRect;\n' +
      shader.fragmentShader.replace(
        '#include <map_fragment>',
        `#include <map_fragment>
        vec2 faceUv = vec2((vMapUv.x - faceRect.x) / faceRect.z, (1.0 - vMapUv.y - faceRect.y) / faceRect.z);
        if (all(greaterThanEqual(faceUv, vec2(0.0))) && all(lessThanEqual(faceUv, vec2(1.0)))) {
          vec4 faceColor = texture2D(faceMap, faceUv);
          diffuseColor.rgb = mix(diffuseColor.rgb, faceColor.rgb, faceColor.a);
        }`,
      );
  };
  material.customProgramCacheKey = () => 'torneo_face_slot';
  material.needsUpdate = true;
}

interface Playing {
  readonly clip: ClipName;
  readonly action: AnimationAction;
  /** Scene time when it became current. */
  readonly since: number;
}

export interface Fighter3d {
  readonly root: Object3D;
  /** `angle` turns the fight axis and `depth` steps off it (018). */
  apply(
    fighter: FighterScene,
    pose: Pose,
    t: number,
    flash: number,
    angle?: number,
    depth?: number,
  ): void;
  dispose(): void;
}

export function createFighter(
  botarga: Botarga,
  side: Side,
  tint: BotargaTint | undefined,
  lite = false,
): Fighter3d {
  const root = cloneSkinned(botarga.gltf.scene);
  const materials: BotargaMaterial[] = [];
  const face = { value: (botarga.faces.get('neutral') ?? null) as Texture | null };
  let currentFace: string | null = null;

  root.traverse((node) => {
    const mesh = node as Mesh;
    if (!mesh.isMesh) return;
    // Skinned bounds come from the rest pose; a clip can leave them.
    mesh.frustumCulled = false;
    const source = mesh.material as MeshStandardMaterial;
    const material: BotargaMaterial = lite
      ? new MeshLambertMaterial({ name: source.name, color: source.color, map: source.map })
      : source.clone();
    const extras = (source.userData ?? {}) as { tint_default?: unknown; face_uv_rect?: unknown };
    const custom = source.name === SHORTS ? tint?.shorts : tint?.body;
    const color = custom ?? extras.tint_default;
    if (typeof color === 'string') material.color.set(color);
    if (source.name === FACE) {
      const rect = parseFaceRect(extras.face_uv_rect);
      if (rect !== null && face.value !== null) addFaceSlot(material, rect, face);
    }
    mesh.material = material;
    materials.push(material);
  });

  const mixer = new AnimationMixer(root);
  const actions = new Map<ClipName, AnimationAction>();
  const actionFor = (clip: ClipName): AnimationAction => {
    let action = actions.get(clip);
    if (action === undefined) {
      const source = botarga.clips.get(clip);
      if (source === undefined) throw new Error(`missing clip ${clip}`);
      action = mixer.clipAction(source);
      action.setLoop(LoopOnce, 1);
      action.clampWhenFinished = true;
      actions.set(clip, action);
    }
    return action;
  };

  let current: Playing | null = null;
  let previous: Playing | null = null;
  /** Scene time of the previous `apply`: a jump (a seek, a late join) has no fade. */
  let lastT: number | null = null;
  const flashColor = new Color();

  const setTime = (action: AnimationAction, time: number): void => {
    action.time = Math.max(0, Math.min(time, action.getClip().duration - END_EPSILON));
  };

  return {
    root,
    apply(fighter, pose, t, flash, angle = 0, depth = 0) {
      const at = arenaPoint(fighter.x, GROUND_Y, angle, depth);
      root.position.set(at.x, 0, at.z);
      // Facing the rival along the turned axis (`angle`, 018), still a little to the camera.
      root.rotation.y =
        angle + (side === 'a' ? Math.PI / 2 - TURN_TO_CAMERA : TURN_TO_CAMERA - Math.PI / 2);

      if (current === null || current.clip !== pose.clip) {
        if (previous !== null && previous.clip !== pose.clip) previous.action.stop();
        previous = current;
        const action = actionFor(pose.clip);
        action.reset();
        action.play();
        action.paused = true;
        const jumped = lastT === null || t < lastT || t - lastT > FADE_MS;
        current = { clip: pose.clip, action, since: jumped ? t - FADE_MS : t };
      }
      setTime(current.action, pose.time);
      lastT = t;
      const p = t < current.since ? 1 : Math.min(1, (t - current.since) / FADE_MS);
      current.action.setEffectiveWeight(p);
      if (previous !== null) {
        if (p >= 1) {
          previous.action.stop();
          previous = null;
        } else {
          previous.action.setEffectiveWeight(1 - p);
        }
      }
      mixer.update(0);

      if (pose.face !== currentFace) {
        const texture = botarga.faces.get(pose.face) ?? botarga.faces.get('neutral') ?? null;
        if (texture !== null) face.value = texture;
        currentFace = pose.face;
      }
      flashColor.setScalar(Math.max(0, Math.min(1, flash)) * FLASH_STRENGTH);
      for (const material of materials) material.emissive.copy(flashColor);
    },
    dispose() {
      mixer.stopAllAction();
      mixer.uncacheRoot(root);
      for (const material of materials) (material as Material).dispose();
    },
  };
}
