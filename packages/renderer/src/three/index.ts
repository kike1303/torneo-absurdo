// @torneo/renderer/three: the 3D arena (013). It never imports `src/index.ts`, so Pixi stays out of
// its chunk; the host loads it with `import()` like the Pixi one (RF-A17).
import { paceAssets } from '../model/pacing.ts';
import { createPlayer } from '../player/player.ts';
import type { RendererHandle } from '../player/types.ts';
import { createThreeStage } from './stage.ts';
import type { Renderer3dOptions } from './types.ts';

export type {
  AccessoryModel,
  BotargaModel,
  BotargaTint,
  FighterLook,
  Renderer3dOptions,
  SkinModel,
} from './types.ts';
export { parseAccessoryManifest, parseSkinManifest, type SkinManifest } from '../look/manifest.ts';
export {
  BODY_SLOTS,
  TINT_SLOTS,
  type AccessoryManifest,
  type BodyFit,
  type BodySlot,
  type ColorChoice,
  type ColorZones,
  type TintSlot,
} from '../look/types.ts';
export {
  FACE_CODES,
  type FaceCode,
  type FighterCosmetics,
  type IdleClip,
  type WinClip,
} from '../model/pose.ts';

/**
 * Draws a timeline in `container` with Three.js and the generic botarga. Same handle as
 * `createRenderer`; `load` also rejects with `RendererError('asset_load_failed')` if the GLB or a
 * face cannot be loaded. The manifests get the botarga's own pace (`paceAssets`, 017).
 */
export function createRenderer3d(
  container: HTMLElement,
  options: Renderer3dOptions,
): RendererHandle {
  const handle = createPlayer(options, createThreeStage(container, options.model));
  return {
    ...handle,
    load: (timeline, assets) => handle.load(timeline, paceAssets(assets)),
    loadLive: (timeline, assets) => handle.loadLive(timeline, paceAssets(assets)),
  };
}
