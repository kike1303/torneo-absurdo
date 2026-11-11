// The materials of one fighter (013, 022): every fighter owns its copies, so a tint, a face or a
// recolored texture never leaks to the other side.
import { MeshLambertMaterial, Source, type MeshStandardMaterial, type Texture } from 'three';

/** PBR as exported, or diffuse only (`lite`): a far smaller shader, for software WebGL. */
export type BotargaMaterial = MeshStandardMaterial | MeshLambertMaterial;

export function ownMaterial(source: MeshStandardMaterial, lite: boolean): BotargaMaterial {
  return lite
    ? new MeshLambertMaterial({ name: source.name, color: source.color, map: source.map })
    : source.clone();
}

/** The texels of a texture's image, as it is stored (no flip). `null` without a 2D canvas. */
export function texelsOf(
  image: CanvasImageSource,
  width: number,
  height: number,
  smooth = true,
): ImageData | null {
  const canvas = document.createElement('canvas');
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext('2d', { willReadFrequently: true });
  if (context === null) return null;
  context.imageSmoothingEnabled = smooth;
  context.drawImage(image, 0, 0, width, height);
  return context.getImageData(0, 0, width, height);
}

/** A texture with the settings of `like` and its own pixels. */
export function textureWith(like: Texture, pixels: ImageData): Texture | null {
  const canvas = document.createElement('canvas');
  canvas.width = pixels.width;
  canvas.height = pixels.height;
  const context = canvas.getContext('2d');
  if (context === null) return null;
  context.putImageData(pixels, 0, 0);
  const copy = like.clone();
  // `clone` shares the source: a new one, or the original would change too.
  copy.source = new Source(canvas);
  copy.needsUpdate = true;
  return copy;
}
