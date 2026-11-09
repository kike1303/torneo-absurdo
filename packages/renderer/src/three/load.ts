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
import type { BotargaModel } from './types.ts';

export interface Botarga {
  readonly gltf: GLTF;
  readonly clips: ReadonlyMap<string, AnimationClip>;
  readonly durations: ClipDurations;
  readonly faces: ReadonlyMap<FaceCode, Texture>;
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

/** Rejects with `RendererError('asset_load_failed')`. */
export async function loadBotarga(model: BotargaModel): Promise<Botarga> {
  try {
    const loader = new GLTFLoader().register(imagesThroughImg);
    loader.setMeshoptDecoder(MeshoptDecoder);
    const textures = new TextureLoader();
    const faceCodes = FACE_CODES.filter((code) => model.faces[code] !== undefined);
    const [gltf, ...faceTextures] = await Promise.all([
      loader.loadAsync(model.glb),
      ...faceCodes.map((code) => loadFace(textures, model.faces[code] as string)),
    ]);
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
    const faces = new Map(faceCodes.map((code, i) => [code, faceTextures[i] as Texture]));
    return { gltf, clips, durations, faces };
  } catch (cause) {
    throw new RendererError('asset_load_failed', { cause });
  }
}
