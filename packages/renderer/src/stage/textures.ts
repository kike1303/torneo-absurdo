// Sprite sheets through Pixi Assets, same origin only and without workers (ADR-0012 B4.2).
import { Assets, Rectangle, Texture } from 'pixi.js';
import type { SpriteManifest } from '../manifest/types.ts';
import { RendererError } from '../player/errors.ts';

export interface FighterTextures {
  /** One texture per manifest frame. */
  readonly frames: readonly Texture[];
  /** Anchor of each frame: its pivot (the feet), so the mirror flips around it. */
  readonly anchors: readonly { readonly x: number; readonly y: number }[];
}

export interface TextureSet {
  readonly byAsset: ReadonlyMap<string, FighterTextures>;
  readonly urls: readonly string[];
}

export const EMPTY_TEXTURES: TextureSet = { byAsset: new Map(), urls: [] };

let preferencesSet = false;

/** Workers would be created from `blob:` URLs, which the CSP blocks (ADR-0012 B4.2). */
export function disableWorkers(): void {
  if (preferencesSet) return;
  Assets.setPreferences({ preferWorkers: false });
  preferencesSet = true;
}

export async function loadTextures(
  manifests: ReadonlyMap<string, SpriteManifest>,
  resolveAsset: (assetId: string, file: string) => string,
): Promise<TextureSet> {
  disableWorkers();
  const urls: string[] = [];
  const byAsset = new Map<string, FighterTextures>();
  try {
    for (const [id, manifest] of manifests) {
      const sheets: Texture[] = [];
      for (const sheet of manifest.sheets) {
        const url = resolveAsset(id, sheet.file);
        urls.push(url);
        const texture = await Assets.load<Texture>({ src: url, parser: 'texture' });
        if (texture.width !== sheet.width || texture.height !== sheet.height) {
          throw new RendererError('asset_invalid');
        }
        if (manifest.pixel_art) texture.source.scaleMode = 'nearest';
        sheets.push(texture);
      }
      const frames = manifest.frames.map((f) => {
        const sheet = sheets[f.sheet];
        if (sheet === undefined) throw new RendererError('asset_invalid');
        return new Texture({ source: sheet.source, frame: new Rectangle(f.x, f.y, f.w, f.h) });
      });
      const anchors = manifest.frames.map((f) => ({ x: f.pivot_x / f.w, y: f.pivot_y / f.h }));
      byAsset.set(id, { frames, anchors });
    }
  } catch (error) {
    await unloadTextures({ byAsset, urls });
    if (error instanceof RendererError) throw error;
    throw new RendererError('asset_load_failed', { cause: error });
  }
  return { byAsset, urls };
}

export async function unloadTextures(set: TextureSet): Promise<void> {
  for (const fighter of set.byAsset.values()) {
    for (const texture of fighter.frames) texture.destroy(false);
  }
  await Promise.allSettled(set.urls.map((url) => Assets.unload(url)));
}
