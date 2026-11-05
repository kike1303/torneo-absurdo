// @torneo/renderer/three: the 3D arena (013). It never imports `src/index.ts`, so Pixi stays out of
// its chunk; the host loads it with `import()` like the Pixi one (RF-A17).
import { createPlayer } from '../player/player.ts';
import type { RendererHandle } from '../player/types.ts';
import { createThreeStage } from './stage.ts';
import type { Renderer3dOptions } from './types.ts';

export type { BotargaModel, BotargaTint, Renderer3dOptions } from './types.ts';
export { FACE_CODES, type FaceCode } from '../model/pose.ts';

/**
 * Draws a timeline in `container` with Three.js and the generic botarga. Same handle as
 * `createRenderer`; `load` also rejects with `RendererError('asset_load_failed')` if the GLB or a
 * face cannot be loaded.
 */
export function createRenderer3d(
  container: HTMLElement,
  options: Renderer3dOptions,
): RendererHandle {
  return createPlayer(options, createThreeStage(container, options.model));
}
