// Pixi Application (RF-A2, RF-A15, RF-A18): WebGL only, no antialias, resolution ≤ 2.
import { Application, RendererType } from 'pixi.js';
import { RendererError } from '../player/errors.ts';
import type { RendererTheme } from '../player/types.ts';

export const MAX_RESOLUTION = 2;

export async function createApp(
  width: number,
  height: number,
  theme: RendererTheme,
): Promise<Application> {
  const app = new Application();
  try {
    await app.init({
      // Only WebGL: a silent fallback would hide the problem and change the measurement.
      preference: ['webgl'],
      antialias: false,
      resolution: Math.min(globalThis.devicePixelRatio || 1, MAX_RESOLUTION),
      autoDensity: true,
      background: theme.colors.bg,
      width: Math.max(1, width),
      height: Math.max(1, height),
    });
  } catch (error) {
    throw new RendererError('webgl_unavailable', { cause: error });
  }
  if (app.renderer.type !== RendererType.WEBGL) {
    app.destroy(true);
    throw new RendererError('webgl_unavailable');
  }
  return app;
}
