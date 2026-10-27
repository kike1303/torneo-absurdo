// The renderer draws no text before the theme fonts load (ADR-0009 D7, RF-A13).
import type { RendererTheme } from '../player/types.ts';

export async function loadFonts(theme: RendererTheme): Promise<void> {
  if (typeof document === 'undefined' || document.fonts === undefined) return;
  const families = [...new Set([theme.fonts.sans, theme.fonts.display])];
  // A family that fails to load falls back to the next one of its stack; never block on it.
  await Promise.allSettled(families.map((family) => document.fonts.load(`16px ${family}`)));
}
