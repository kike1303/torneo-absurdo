// Public types of the handle (ADR-0009 D7, extended by RF-A11) and the `Stage` the player
// drives (plan-a2 decision 1). No Pixi and no DOM here.
import type { SpriteManifest } from '../manifest/types.ts';
import type { BannerCode, Phase, Scene } from '../scene/types.ts';
import type { FrameStats } from '../stats/frame-stats.ts';
import type { Cue, CueType, Ending, Side } from '../timeline/types.ts';

/** Colors as 0xRRGGBB, because Pixi does not parse OKLCH colors; see ADR-0009 D7. */
export interface RendererTheme {
  readonly colors: {
    readonly bg: number;
    readonly hp: number;
    readonly hpLost: number;
    readonly crit: number;
    readonly text: number;
  };
  readonly fonts: { readonly sans: string; readonly display: string };
}

export interface RendererLabels {
  /** Already translated by the host. */
  readonly names: Readonly<Record<Side, string>>;
  /** One text per banner code (`fight`, `ko` and `time`), already translated. */
  readonly banners: Readonly<Record<BannerCode, string>>;
}

export interface RendererOptions {
  readonly theme: RendererTheme;
  readonly reducedMotion: boolean;
  /** Time in ms on the timeline's time base; the renderer never counts frames (RF-A11). */
  readonly clock: () => number;
  readonly labels: RendererLabels;
  /** Formats damage numbers (`Intl`, from the host). */
  readonly formatNumber: (n: number) => string;
  /** URL of each file of a manifest (Vite hashes file names). */
  readonly resolveAsset: (assetId: string, file: string) => string;
}

export type CueListener = (cue: Cue, index: number) => void;

export type PlayerState = 'ready' | 'playing' | 'paused' | 'finished';
export type StateListener = (state: PlayerState) => void;

export interface RendererSnapshot {
  readonly t: number;
  readonly phase: Phase;
  readonly winner: Side | null;
  /** `ko`, `decision` or `draw` from the closing cue on; null before (005 RF7). */
  readonly ending: Ending | null;
  readonly hp: Readonly<Record<Side, number>>;
  /** The last cue with `at_ms ≤ t`, or null before the first one. */
  readonly lastCue: { readonly index: number; readonly type: CueType } | null;
}

export interface RendererHandle {
  /**
   * Validates the timeline and the manifests (by asset id) and loads the sheets. Both are
   * untrusted JSON; rejects with a `RendererError`.
   */
  load(timeline: unknown, assets: ReadonlyMap<string, unknown>): Promise<void>;
  play(): void;
  pause(): void;
  seek(ms: number): void;
  setTheme(theme: RendererTheme): void;
  setReducedMotion(flag: boolean): void;
  /** Returns the unsubscribe function. */
  onCue(listener: CueListener): () => void;
  /** Returns the unsubscribe function. */
  onState(listener: StateListener): () => void;
  /** Values of the last applied scene, or null before `load`. */
  snapshot(): RendererSnapshot | null;
  getFrameStats(): FrameStats;
  destroy(): void;
}

export interface StageSetup {
  readonly theme: RendererTheme;
  readonly labels: RendererLabels;
  readonly formatNumber: (n: number) => string;
  /** Called once per display frame with the real interval since the previous one. */
  readonly onFrame: (elapsedMs: number) => void;
}

/** What the player needs from a drawing layer: Pixi in production, a fake in the tests. */
export interface Stage {
  /** Rejects with `RendererError('webgl_unavailable')`. */
  init(setup: StageSetup): Promise<void>;
  /** Rejects with `RendererError('asset_load_failed')`. Replaces what was loaded before. */
  loadAssets(
    manifests: ReadonlyMap<string, SpriteManifest>,
    resolveAsset: (assetId: string, file: string) => string,
  ): Promise<void>;
  apply(scene: Scene): void;
  setTheme(theme: RendererTheme): void;
  destroy(): void;
}
