// What a fighter wears over its body (022): accessories hung from a bone or skinned to the
// skeleton, their colors, the body skin they hide and the zone recolor of a skin. Everything is
// worked out once, in the rest pose, when the fighter is created.
import {
  BufferAttribute,
  Euler,
  MathUtils,
  Matrix4,
  Quaternion,
  Skeleton,
  Vector3,
  type Bone,
  type BufferGeometry,
  type Mesh,
  type MeshStandardMaterial,
  type Object3D,
  type SkinnedMesh,
  type Texture,
} from 'three';
import { clone as cloneSkinned } from 'three/addons/utils/SkeletonUtils.js';
import { parseHex, type Rgb } from '../look/color.ts';
import { createGarment, visibleTriangles } from '../look/mask.ts';
import { remapPattern } from '../look/pattern.ts';
import {
  TINT_SLOTS,
  type BodyFit,
  type ColorChoice,
  type SlotFit,
  type TintSlot,
} from '../look/types.ts';
import { islandIndex, shiftPixels, zoneTable } from '../look/zones.ts';
import type { LoadedAccessory } from './load.ts';
import { ownMaterial, texelsOf, textureWith, type BotargaMaterial } from './materials.ts';
import type { SkinModel } from './types.ts';

/** Angular speed of a propeller, in radians per second. */
const SPIN = 9;
const SPINNER = /propeller$/i;
const NO_FIT: SlotFit = { scale: [1, 1, 1], offset: [0, 0, 0] };

export interface Wardrobe {
  /** The materials of the accessories, for the hit flash. */
  readonly materials: readonly BotargaMaterial[];
  /** Presentation that only depends on the scene time (a propeller). */
  update(t: number): void;
  dispose(): void;
}

interface Extras {
  readonly tint_default?: unknown;
  readonly tint_slot?: unknown;
  readonly tint_shade?: unknown;
  readonly tint_pattern?: unknown;
}

const isSlot = (value: unknown): value is TintSlot =>
  (TINT_SLOTS as readonly unknown[]).includes(value);

function meshesOf(root: Object3D): Mesh[] {
  const meshes: Mesh[] = [];
  root.traverse((node) => {
    if ((node as Mesh).isMesh) meshes.push(node as Mesh);
  });
  return meshes;
}

const isSkinned = (mesh: Mesh): mesh is SkinnedMesh => (mesh as SkinnedMesh).isSkinnedMesh === true;

/**
 * Gives every mesh of `node` its own materials with the colors of this fighter: each slot takes
 * the chosen color or its default, a lining is shaded and a pattern is redrawn between its two.
 */
function colorAccessory(
  node: Object3D,
  colors: Readonly<Partial<Record<TintSlot, string>>> | undefined,
  lite: boolean,
  owned: Map<MeshStandardMaterial, BotargaMaterial>,
  textures: Texture[],
): void {
  const meshes = meshesOf(node);
  const defaults = new Map<TintSlot, Rgb>();
  for (const mesh of meshes) {
    const extras = ((mesh.material as MeshStandardMaterial).userData ?? {}) as Extras;
    const color = parseHex(extras.tint_default);
    if (isSlot(extras.tint_slot) && color !== null && !defaults.has(extras.tint_slot))
      defaults.set(extras.tint_slot, color);
  }
  const worn = (slot: TintSlot): Rgb | undefined => parseHex(colors?.[slot]) ?? defaults.get(slot);

  for (const mesh of meshes) {
    mesh.frustumCulled = false;
    const source = mesh.material as MeshStandardMaterial;
    let material = owned.get(source);
    if (material === undefined) {
      material = ownMaterial(source, lite);
      owned.set(source, material);
      const extras = (source.userData ?? {}) as Extras;
      const color = isSlot(extras.tint_slot)
        ? worn(extras.tint_slot)
        : parseHex(extras.tint_default);
      if (color !== undefined && color !== null) {
        const shade = typeof extras.tint_shade === 'number' ? extras.tint_shade : 1;
        // `tint_default` is written in sRGB, as the body's.
        material.color.setRGB(color[0], color[1], color[2], 'srgb').multiplyScalar(shade);
      }
      const pattern = typeof extras.tint_pattern === 'string' ? extras.tint_pattern.split(',') : [];
      const [first, second] = pattern;
      if (source.map !== null && isSlot(first) && isSlot(second)) {
        const from = [defaults.get(first), defaults.get(second)] as const;
        const to = [worn(first), worn(second)] as const;
        const changed = from[0] !== to[0] || from[1] !== to[1];
        if (changed && from[0] && from[1] && to[0] && to[1]) {
          const image = source.map.image as HTMLImageElement;
          const texels = texelsOf(image, image.width, image.height);
          if (texels !== null) {
            const out = new ImageData(texels.width, texels.height);
            remapPattern(texels.data, out.data, [from[0], from[1]], [to[0], to[1]]);
            const texture = textureWith(source.map, out);
            if (texture !== null) {
              material.map = texture;
              textures.push(texture);
            }
          }
        }
      }
    }
    mesh.material = material;
  }
}

interface RestNode {
  readonly node: Object3D;
  readonly world: Matrix4;
}

/** Where an accessory sits in the fighter's space, from the rest position of its anchor. */
function placement(
  anchor: Matrix4,
  body: SlotFit,
  fit: { y?: number; z?: number; scale?: number; tilt?: number } | undefined,
  mirror: boolean,
): Matrix4 {
  const s = fit?.scale ?? 1;
  const at = new Vector3().setFromMatrixPosition(anchor);
  at.x += body.offset[0] * (mirror ? -1 : 1);
  at.y += body.offset[1] + (fit?.y ?? 0) * body.scale[1];
  at.z += body.offset[2] + (fit?.z ?? 0) * body.scale[2];
  const tilt = new Quaternion().setFromEuler(new Euler(-MathUtils.degToRad(fit?.tilt ?? 0), 0, 0));
  const sx = body.scale[0] * s;
  return new Matrix4().compose(
    at,
    tilt,
    new Vector3(mirror ? -sx : sx, body.scale[1] * s, body.scale[2] * s),
  );
}

interface BodyRest {
  readonly mesh: SkinnedMesh;
  /** Rest position of each vertex, in the fighter's space. */
  readonly position: Float32Array;
  /** Name of the bone each vertex follows most. */
  readonly owner: readonly string[];
  readonly covered: Uint8Array;
}

function captureBody(mesh: SkinnedMesh): BodyRest {
  const { skinIndex, skinWeight, position } = mesh.geometry.attributes;
  const count = position?.count ?? 0;
  const out = new Float32Array(count * 3);
  const owner: string[] = [];
  const p = new Vector3();
  for (let i = 0; i < count; i++) {
    mesh.getVertexPosition(i, p).applyMatrix4(mesh.matrixWorld);
    p.toArray(out, i * 3);
    let best = 0;
    if (skinWeight !== undefined) {
      for (let k = 1; k < 4; k++)
        if (skinWeight.getComponent(i, k) > skinWeight.getComponent(i, best)) best = k;
    }
    owner.push(mesh.skeleton.bones[skinIndex?.getComponent(i, best) ?? 0]?.name ?? '');
  }
  return { mesh, position: out, owner, covered: new Uint8Array(count) };
}

/**
 * Dresses `root` (a fresh clone, still in its rest pose and at the origin). `fit` is how its body
 * differs from the base botarga, by slot.
 */
export function dress(
  root: Object3D,
  fit: BodyFit | undefined,
  accessories: readonly LoadedAccessory[],
  lite: boolean,
): Wardrobe {
  const materials: BotargaMaterial[] = [];
  const textures: Texture[] = [];
  const geometries: BufferGeometry[] = [];
  const skeletons: Skeleton[] = [];
  const spinners: { node: Object3D; base: number }[] = [];
  if (accessories.length === 0) {
    return { materials, update: () => undefined, dispose: () => undefined };
  }

  root.updateMatrixWorld(true);
  const rest: RestNode[] = [];
  root.traverse((node) => {
    if (node.name !== '') rest.push({ node, world: node.matrixWorld.clone() });
  });
  const byName = new Map(rest.map((entry) => [entry.node.name, entry]));
  const find = (suffix: string): RestNode | undefined =>
    rest.find((entry) => entry.node.name.endsWith(suffix));
  const bodies = meshesOf(root).filter(isSkinned);
  let bodyRest: BodyRest[] | null = null;

  for (const { model, gltf } of accessories) {
    const { manifest } = model;
    const body = fit?.[manifest.slot] ?? NO_FIT;
    const owned = new Map<MeshStandardMaterial, BotargaMaterial>();
    const skinned = meshesOf(gltf.scene).some(isSkinned);

    for (const attach of manifest.attach) {
      const bone = find(attach.bone);
      if (bone === undefined) continue;
      const anchor = (attach.anchor === undefined ? undefined : find(attach.anchor)) ?? bone;
      const placed = placement(anchor.world, body, manifest.fit, attach.mirror === true);

      if (!skinned) {
        const node = gltf.scene.clone(true);
        colorAccessory(node, model.colors, lite, owned, textures);
        node.matrixAutoUpdate = false;
        node.matrix.copy(bone.world).invert().multiply(placed);
        bone.node.add(node);
        node.traverse((child) => {
          if (SPINNER.test(child.name)) spinners.push({ node: child, base: child.rotation.y });
        });
        continue;
      }

      // A skinned garment brings stand-in bones: it is bound again to the fighter's bones of the
      // same name, keeping the transform its own bind applied to its vertices.
      const node = cloneSkinned(gltf.scene);
      node.updateMatrixWorld(true);
      const garmentMeshes = meshesOf(node).filter(isSkinned);
      const bound: { mesh: SkinnedMesh; toModel: Matrix4; joints: string[] }[] = [];
      for (const mesh of garmentMeshes) {
        const first = mesh.skeleton.bones[0];
        const inverse = mesh.skeleton.boneInverses[0];
        const joints = mesh.skeleton.bones.map((b) => b.name);
        const targets = joints.map((name) => byName.get(name));
        if (first === undefined || inverse === undefined || targets.includes(undefined)) continue;
        const toModel = first.matrixWorld.clone().multiply(inverse).multiply(mesh.bindMatrix);
        const bones = targets as RestNode[];
        const standIns = mesh.skeleton.bones;
        const skeleton = new Skeleton(
          bones.map((b) => b.node as Bone),
          bones.map((b) => b.world.clone().invert()),
        );
        mesh.bind(skeleton, placed.clone().multiply(toModel));
        skeletons.push(skeleton);
        // The stand-ins carry the names of the real bones: out of the tree, so a clip never
        // finds them instead.
        for (const standIn of standIns) standIn.removeFromParent();
        bound.push({ mesh, toModel, joints });
      }
      if (bound.length !== garmentMeshes.length) continue;
      colorAccessory(node, model.colors, lite, owned, textures);
      root.add(node);

      if (manifest.hides !== true) continue;
      bodyRest ??= bodies.map(captureBody);
      const toGarment = placed.clone().invert();
      const p = new Vector3();
      for (const { mesh, toModel, joints } of bound) {
        const { position } = mesh.geometry.attributes;
        const index = mesh.geometry.index;
        if (position === undefined || index === null) continue;
        const triangles = new Float32Array(index.count * 3);
        for (let i = 0; i < index.count; i++) {
          p.fromBufferAttribute(position, index.getX(i)).applyMatrix4(toModel);
          p.toArray(triangles, i * 3);
        }
        const garment = createGarment(triangles);
        const follows = new Set(joints);
        for (const skin of bodyRest) {
          for (let i = 0; i < skin.covered.length; i++) {
            if (skin.covered[i] === 1 || !follows.has(skin.owner[i] ?? '')) continue;
            p.fromArray(skin.position, i * 3).applyMatrix4(toGarment);
            if (garment.covers(p.x, p.y, p.z)) skin.covered[i] = 1;
          }
        }
      }
    }
    materials.push(...owned.values());
  }

  for (const skin of bodyRest ?? []) {
    const index = skin.mesh.geometry.index;
    if (index === null) continue;
    const visible = visibleTriangles(index.array, skin.covered);
    if (visible === null) continue;
    // Its own geometry: the other side shares the original and may wear nothing.
    const geometry = skin.mesh.geometry.clone();
    geometry.setIndex(new BufferAttribute(visible, 1));
    skin.mesh.geometry = geometry;
    geometries.push(geometry);
  }

  return {
    materials,
    update(t) {
      for (const { node, base } of spinners) node.rotation.y = base + (t / 1000) * SPIN;
    },
    dispose() {
      for (const texture of textures) texture.dispose();
      for (const geometry of geometries) geometry.dispose();
      for (const skeleton of skeletons) skeleton.dispose();
    },
  };
}

/**
 * Recolors the textures of a skin by zones, for this fighter only. Returns the textures it made,
 * for the fighter to dispose; none when nothing is chosen.
 */
export function recolorZones(
  materials: readonly BotargaMaterial[],
  skin: SkinModel,
  colors: ColorChoice | undefined,
  islandImage: HTMLImageElement | null,
): Texture[] {
  const zones = skin.zones;
  if (zones === undefined || colors === undefined) return [];
  const ids = new Set([...zones.zones, ...(zones.islands ?? [])].map((entry) => entry.id));
  if (!Object.keys(colors).some((id) => ids.has(id) && parseHex(colors[id]) !== null)) return [];

  const byMap = new Map<Texture, BotargaMaterial[]>();
  for (const material of materials) {
    if (material.map === null) continue;
    byMap.set(material.map, [...(byMap.get(material.map) ?? []), material]);
  }
  const made: Texture[] = [];
  for (const [map, users] of byMap) {
    const image = map.image as HTMLImageElement;
    const texels = texelsOf(image, image.width, image.height);
    if (texels === null) continue;
    const names = new Set(users.map((material) => material.name));
    const islands =
      islandImage !== null && skin.islandMap !== undefined && names.has(skin.islandMap.material)
        ? (zones.islands ?? [])
        : [];
    const tables = [
      zoneTable(zones.zones, colors, names),
      ...islands.map((island) => zoneTable(zones.zones, colors, names, island)),
    ];
    let islandOf: Uint8Array | undefined;
    if (islands.length > 0 && islandImage !== null && skin.islandMap !== undefined) {
      // Stretched to the texture without smoothing: one island number per texel.
      const gray = texelsOf(islandImage, texels.width, texels.height, false);
      if (gray !== null) islandOf = islandIndex(gray.data, skin.islandMap.step);
    }
    const out = new ImageData(texels.width, texels.height);
    shiftPixels(texels.data, out.data, tables, islandOf);
    const texture = textureWith(map, out);
    if (texture === null) continue;
    for (const material of users) material.map = texture;
    made.push(texture);
  }
  return made;
}
