// What the host gives the 3D layer (013): the URL of the botarga and of each face, and the tint of
// each side. No colors here: they come from the GLB (`extras.tint_default`) or from the host.
import type { FaceCode, FighterCosmetics } from '../model/pose.ts';
import type { RendererOptions } from '../player/types.ts';
import type { Side } from '../timeline/types.ts';

export interface BotargaTint {
  /** CSS color of the body and the face. */
  readonly body?: string;
  /** CSS color of the shorts. */
  readonly shorts?: string;
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
}

export interface Renderer3dOptions extends RendererOptions {
  readonly model: BotargaModel;
}
