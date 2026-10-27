// The structure of an SFF file: headers and nodes, never pixels (002 plan-b decision 5).
import type { ByteReader } from './reader.ts';

export type Compression =
  'pcx' | 'raw' | 'rle8' | 'rle5' | 'lz5' | 'png8' | 'png24' | 'png32' | 'unknown';

export interface SpriteNodeBase {
  readonly index: number;
  readonly group: number;
  readonly image: number;
  readonly axisX: number;
  readonly axisY: number;
  /** Index of the sprite that owns the data when this one is linked (data length 0). */
  readonly linked: number | null;
}

export interface SpriteNodeV1 extends SpriteNodeBase {
  readonly data: ByteReader | null;
  readonly samePalette: boolean;
}

export interface SpriteNodeV2 extends SpriteNodeBase {
  readonly width: number;
  readonly height: number;
  readonly format: number;
  readonly depth: number;
  readonly paletteIndex: number;
  readonly data: ByteReader | null;
}

export interface PaletteNodeV2 {
  readonly index: number;
  readonly colors: number;
  readonly linked: number | null;
  readonly data: ByteReader | null;
}

export interface SffV1 {
  readonly version: 1;
  /** `1.01`, from the version bytes. */
  readonly versionLabel: string;
  readonly sharedPalette: boolean;
  readonly sprites: readonly SpriteNodeV1[];
}

export interface SffV2 {
  readonly version: 2;
  readonly versionLabel: string;
  readonly sprites: readonly SpriteNodeV2[];
  readonly palettes: readonly PaletteNodeV2[];
}

export type SffFile = SffV1 | SffV2;
