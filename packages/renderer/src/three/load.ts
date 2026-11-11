// Loads the botarga GLB and its faces (013). The CSP rules (ADR-0012): no workers, no Draco nor
// KTX2, and the images embedded in the GLB go through `<img>` (`img-src blob:`), because the
// default `ImageBitmapLoader` fetches their `blob:` URL and `connect-src 'self'` blocks that.
// The GLB is compressed with `EXT_meshopt_compression` (TA-70): the decoder is a WASM module
// embedded in Three (`'wasm-unsafe-eval'` is already in `script-src`), decoding on the main thread
// (`useWorkers` is never called) in a few milliseconds.
import { SRGBColorSpace, TextureLoader, type AnimationClip, type Texture } from 'three';
import { MeshoptDecoder } from 'three/addons/libs/meshopt_decoder.module.js';
import { GLTFLoader, type GLTF, type GLTFParser } from 'three/addons/loaders/GLTFLoader.js';
import {
  CLIP_NAMES,
  FACE_CODES,
  OPTIONAL_CLIP_NAMES,
  type ClipDurations,
  type ClipName,
  type FaceCode,
} from '../model/pose.ts';
import { RendererError } from '../player/errors.ts';
import type { Side } from '../timeline/types.ts';
import type { AccessoryModel, BotargaModel, FighterLook, SkinModel } from './types.ts';

/** A body that can fight: the base botarga or a skin (022), with its own clips. */
export interface Body {
  readonly gltf: GLTF;
  readonly clips: ReadonlyMap<string, AnimationClip>;
  readonly durations: ClipDurations;
}

export interface LoadedAccessory {
  readonly model: AccessoryModel;
  readonly gltf: GLTF;
}

/** What one side fights in (022). */
export interface Outfit {
  readonly body: Body;
  /** Absent on the base botarga. */
  readonly skin?: SkinModel;
  readonly islandImage: HTMLImageElement | null;
  readonly look: FighterLook;
  readonly accessories: readonly LoadedAccessory[];
}

export interface Botarga extends Body {
  readonly faces: ReadonlyMap<FaceCode, Texture>;
  readonly outfits: Readonly<Record<Side, Outfit>>;
  /** Every GLB loaded besides the base one (skins and accessories), to dispose. */
  readonly extras: readonly GLTF[];
}

function imagesThroughImg(parser: GLTFParser) {
  // `textureLoader` is not in the public types; it is the loader the parser uses for every image.
  (parser as unknown as { textureLoader: TextureLoader }).textureLoader = new TextureLoader(
    parser.options.manager,
  );
  return { name: 'torneo_images_through_img' };
}

async function loadFace(loader: TextureLoader, url: string): Promise<Texture> {
  const texture = await loader.loadAsync(url);
  texture.colorSpace = SRGBColorSpace;
  return texture;
}

/** Throws when a clip the pose needs is missing: a skin must bring them all, as the base does. */
function bodyOf(gltf: GLTF): Body {
  const clips = new Map(gltf.animations.map((clip) => [clip.name, clip]));
  const durations = {} as Record<ClipName, number>;
  for (const name of CLIP_NAMES) {
    const clip = clips.get(name);
    if (clip === undefined) throw new Error(`missing clip ${name}`);
    durations[name] = clip.duration;
  }
  for (const name of OPTIONAL_CLIP_NAMES) {
    const clip = clips.get(name);
    if (clip !== undefined) durations[name] = clip.duration;
  }
  return { gltf, clips, durations };
}

/** Rejects with `RendererError('asset_load_failed')`. */
export async function loadBotarga(model: BotargaModel): Promise<Botarga> {
  try {
    const loader = new GLTFLoader().register(imagesThroughImg);
    loader.setMeshoptDecoder(MeshoptDecoder);
    const textures = new TextureLoader();
    const faceCodes = FACE_CODES.filter((code) => model.faces[code] !== undefined);
    // Each URL once: both sides may wear the same skin or the same accessory.
    const glbs = new Map<string, Promise<GLTF>>([[model.glb, loader.loadAsync(model.glb)]]);
    const glb = (url: string): Promise<GLTF> => {
      let pending = glbs.get(url);
      if (pending === undefined) {
        pending = loader.loadAsync(url);
        glbs.set(url, pending);
      }
      return pending;
    };
    const outfit = async (look: FighterLook): Promise<Outfit> => {
      const island = look.skin?.islandMap?.url;
      const [gltf, islandTexture, ...accessories] = await Promise.all([
        glb(look.skin?.glb ?? model.glb),
        island === undefined ? null : textures.loadAsync(island),
        ...(look.accessories ?? []).map(async (accessory) => ({
          model: accessory,
          gltf: await glb(accessory.glb),
        })),
      ]);
      const islandImage = (islandTexture?.image ?? null) as HTMLImageElement | null;
      islandTexture?.dispose();
      return { body: bodyOf(gltf), skin: look.skin, islandImage, look, accessories };
    };
    const [gltf, a, b, ...faceTextures] = await Promise.all([
      glb(model.glb),
      outfit(model.looks?.a ?? {}),
      outfit(model.looks?.b ?? {}),
      ...faceCodes.map((code) => loadFace(textures, model.faces[code] as string)),
    ]);
    const faces = new Map(faceCodes.map((code, i) => [code, faceTextures[i] as Texture]));
    const extras = (await Promise.all(glbs.values())).filter((other) => other !== gltf);
    return { ...bodyOf(gltf), faces, outfits: { a, b }, extras };
  } catch (cause) {
    throw new RendererError('asset_load_failed', { cause });
  }
}
