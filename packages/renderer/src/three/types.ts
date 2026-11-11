// What the host gives the 3D layer (013): the URL of the botarga and of each face, and the tint of
// each side. No colors here: they come from the GLB (`extras.tint_default`) or from the host.
import type {
  AccessoryManifest,
  BodyFit,
  ColorChoice,
  ColorZones,
  TintSlot,
} from '../look/types.ts';
import type { FaceCode, FighterCosmetics } from '../model/pose.ts';
import type { RendererOptions } from '../player/types.ts';
import type { Side } from '../timeline/types.ts';

export interface BotargaTint {
  /** CSS color of the body and the face. */
  readonly body?: string;
  /** CSS color of the shorts. */
  readonly shorts?: string;
}

/** One accessory a fighter wears (022). */
export interface AccessoryModel {
  /** URL of its GLB, modeled at the scale of the base botarga. */
  readonly glb: string;
  /** Its `accessory.json`. */
  readonly manifest: AccessoryManifest;
  /** `#RRGGBB` by color slot; without one, the default of the GLB. */
  readonly colors?: Readonly<Partial<Record<TintSlot, string>>>;
}

/** A full costume (022): another GLB with the skeleton and the clip names of the base botarga. */
export interface SkinModel {
  readonly glb: string;
  /** How its body differs from the base, for the accessories; without it, the base measures. */
  readonly fit?: BodyFit;
  /** The color zones of its baked texture; without them it cannot be recolored. */
  readonly zones?: ColorZones;
  /** Gray image that numbers the islands of `zones`, for the texture `material` draws. */
  readonly islandMap?: { readonly url: string; readonly material: string; readonly step: number };
}

/** What one side wears (022). Without it, the base botarga with its tint. */
export interface FighterLook {
  readonly skin?: SkinModel;
  /** Colors of the skin, by zone or island id. The base botarga uses `tints` instead. */
  readonly colors?: ColorChoice;
  readonly accessories?: readonly AccessoryModel[];
}

export interface BotargaModel {
  /** URL of `botarga-base.glb` (same origin: the CSP allows nothing else). */
  readonly glb: string;
  /** URL of each face PNG, in the framing of the face template; a missing face shows `neutral`. */
  readonly faces: Readonly<Partial<Record<FaceCode, string>>>;
  /** Without it, or without a field, the GLB default. */
  readonly tints?: Readonly<Partial<Record<Side, BotargaTint>>>;
  /** The idle and victory a player picked (016); without it, the tournament variants. */
  readonly cosmetics?: Readonly<Partial<Record<Side, FighterCosmetics>>>;
  /** Skin, colors and accessories of each side (022). */
  readonly looks?: Readonly<Partial<Record<Side, FighterLook>>>;
}

export interface Renderer3dOptions extends RendererOptions {
  readonly model: BotargaModel;
}
