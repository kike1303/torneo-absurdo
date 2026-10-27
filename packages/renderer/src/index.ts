// @torneo/renderer: public types (ADR-0009 D7, extended by RF-A11) and the pure modules.
// `createRenderer` (the Pixi layer, src/stage/) arrives in phase A2.
import type { SpriteManifest } from './manifest/types.ts';
import type { BannerCode } from './scene/types.ts';
import type { Cue, Side, Timeline } from './timeline/types.ts';

export * from './manifest/index.ts';
export * from './timeline/index.ts';
export * from './scene/index.ts';
export * from './fx/index.ts';

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

export interface FrameStats {
  readonly frames: number;
  readonly fpsMean: number;
  readonly p50: number;
  readonly p95: number;
  readonly p99: number;
  readonly over33: number;
}

export type CueListener = (cue: Cue, index: number) => void;

export interface RendererHandle {
  load(timeline: Timeline, assets: ReadonlyMap<string, SpriteManifest>): Promise<void>;
  play(): void;
  pause(): void;
  seek(ms: number): void;
  setTheme(theme: RendererTheme): void;
  setReducedMotion(flag: boolean): void;
  /** Returns the unsubscribe function. */
  onCue(listener: CueListener): () => void;
  getFrameStats(): FrameStats;
  destroy(): void;
}
