// How much the 3D stage draws (013, 015 review): a device that cannot keep up first draws fewer
// pixels and then fewer frames, instead of making the page wait for its GPU. Each drawn frame on a
// slow (or software) GPU holds the next `requestAnimationFrame` until it is shown; with several
// stages at 60 fps that left a page seconds between frames. A frame that is not drawn costs the
// GPU nothing, so the page keeps its clock, cues and events. Pure: no Three, no DOM.

/** Smoothed frame interval over which the pacer steps down, at most once per `STEP_COOLDOWN_MS`. */
export const SLOW_FRAME_MS = 40;
export const STEP_COOLDOWN_MS = 1000;
export const RATIO_STEP = 0.75;
export const MIN_PIXEL_RATIO = 0.5;
/** Steps of the shortest time between two drawn frames, once the pixel ratio is at its minimum. */
export const FRAME_GAPS_MS = [0, 50, 100, 200] as const;
/** Software WebGL starts here: the minimum pixel ratio and 100 ms between drawn frames (8 to 10 a
 * second, with the display's refresh). */
export const SOFTWARE_FRAME_MS = 100;
/**
 * A gap this long is a stall. A lone one says nothing about the GPU (a tab shown again, a window
 * restored, a shader compiled once): only a stall right after another counts (review of #93).
 */
export const STALL_MS = 250;
/** A counted gap weighs at most this much, so one stall does not step twice. */
const MAX_COUNTED_MS = 1000;
const SMOOTHING = 0.1;

export interface Pacer {
  readonly pixelRatio: number;
  /** Shortest time between two drawn frames. */
  readonly minFrameMs: number;
  /**
   * One frame: `gap` ms since the previous one (0 the first time), `hidden` when the page is not
   * shown (a background tab: its gaps say nothing about the GPU). Returns whether the pixel ratio
   * changed.
   */
  observe(gap: number, hidden: boolean): boolean;
  /** Whether to draw at `time` (ms, the clock of `requestAnimationFrame`); records the draw. */
  draw(time: number): boolean;
}

export function createPacer(start: {
  readonly pixelRatio: number;
  readonly software: boolean;
}): Pacer {
  let pixelRatio = start.software ? MIN_PIXEL_RATIO : Math.max(MIN_PIXEL_RATIO, start.pixelRatio);
  let minFrameMs = start.software ? SOFTWARE_FRAME_MS : 0;
  let smoothed = 0;
  let sinceStep = 0;
  let lastDraw: number | null = null;
  let stalls = 0;

  const stepDown = (): boolean => {
    if (pixelRatio > MIN_PIXEL_RATIO) {
      pixelRatio = Math.max(MIN_PIXEL_RATIO, pixelRatio * RATIO_STEP);
      return true;
    }
    const next = FRAME_GAPS_MS.find((ms) => ms > minFrameMs);
    if (next !== undefined) minFrameMs = next;
    return false;
  };

  return {
    get pixelRatio() {
      return pixelRatio;
    },
    get minFrameMs() {
      return minFrameMs;
    },
    observe(gap, hidden) {
      if (gap <= 0 || hidden) return false;
      stalls = gap > STALL_MS ? stalls + 1 : 0;
      if (stalls === 1) return false;
      const counted = Math.min(gap, MAX_COUNTED_MS);
      smoothed += (counted - smoothed) * SMOOTHING;
      sinceStep += counted;
      if (smoothed <= SLOW_FRAME_MS || sinceStep <= STEP_COOLDOWN_MS) return false;
      sinceStep = 0;
      smoothed = 0;
      return stepDown();
    },
    draw(time) {
      if (lastDraw !== null && time - lastDraw < minFrameMs) return false;
      lastDraw = time;
      return true;
    },
  };
}
