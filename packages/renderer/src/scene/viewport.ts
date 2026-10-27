// Letterbox of the 640×360 logical stage in the container (RF-A15). Pure.
import { STAGE_HEIGHT, STAGE_WIDTH } from './types.ts';
import type { Viewport } from './hud.ts';

export interface Letterbox {
  /** CSS px per logical px. */
  readonly scale: number;
  /** Top-left corner of the stage in CSS px of the container. */
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

/** The largest scale that shows the whole stage, centered; never crops it. */
export function letterbox(viewport: Viewport): Letterbox {
  const w = Math.max(0, viewport.width);
  const h = Math.max(0, viewport.height);
  const scale = Math.min(w / STAGE_WIDTH, h / STAGE_HEIGHT);
  const width = STAGE_WIDTH * scale;
  const height = STAGE_HEIGHT * scale;
  return { scale, x: (w - width) / 2, y: (h - height) / 2, width, height };
}

/** Logical font size that renders at least `minCssPx` CSS px after the letterbox scale. */
export function logicalFontPx(cssPx: number, scale: number): number {
  return scale > 0 ? cssPx / scale : cssPx;
}
