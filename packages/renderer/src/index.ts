// pixi.js/unsafe-eval must run before any renderer exists: shader generation and uniform sync
// without `new Function`, so the CSP stays as it is (ADR-0012 B4.2, RF-A18). Keep it first.
import 'pixi.js/unsafe-eval';
// @torneo/renderer: `createRenderer` (the Pixi layer, src/stage/), the public types (ADR-0009
// D7, extended by RF-A11) and the pure modules.
import { createPlayer } from './player/player.ts';
import type { RendererHandle, RendererOptions } from './player/types.ts';
import { createPixiStage } from './stage/index.ts';

export * from './manifest/index.ts';
export * from './timeline/index.ts';
export * from './scene/index.ts';
export * from './fx/index.ts';
export * from './player/index.ts';
export {
  EMPTY_STATS,
  FRAME_BUFFER,
  SLOW_FRAME_MS,
  createFrameRecorder,
  percentile,
  type FrameRecorder,
  type FrameStats,
} from './stats/frame-stats.ts';

/**
 * Draws a timeline in `container` with Pixi. The handle starts the WebGL renderer right away;
 * `load` rejects with `RendererError('webgl_unavailable')` if it cannot.
 */
export function createRenderer(container: HTMLElement, options: RendererOptions): RendererHandle {
  return createPlayer(options, createPixiStage(container));
}
