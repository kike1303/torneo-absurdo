import { describe, expect, it, vi } from 'vitest';
import type { SpriteManifest } from '../src/manifest/types.ts';

// PR #40 review: a URL whose acquire failed never took a reference, so it is never released
// (a release would drop the reference of another renderer that shares the sheet).
const calls = vi.hoisted(() => ({ acquire: [] as string[], release: [] as string[] }));

vi.mock('pixi.js', () => ({
  Assets: { setPreferences: () => {}, load: async () => ({}), unload: async () => {} },
  Rectangle: class {},
  Texture: class {
    destroy(): void {}
  },
}));

vi.mock('../src/stage/shared-cache.ts', () => ({
  createSharedCache: () => ({
    acquire: async (url: string) => {
      calls.acquire.push(url);
      if (url.endsWith('bad.png')) throw new Error('boom');
      return { width: 8, height: 8, source: {} };
    },
    release: async (url: string) => {
      calls.release.push(url);
    },
  }),
}));

const { loadTextures } = await import('../src/stage/textures.ts');

function manifest(files: string[]): SpriteManifest {
  return {
    format: 'torneo.sprites',
    format_version: 1,
    id: 'x',
    pixel_art: true,
    sheets: files.map((file) => ({ file, width: 8, height: 8 })),
    frames: [],
    animations: {} as SpriteManifest['animations'],
  };
}

describe('loadTextures', () => {
  it('releases only the sheets it acquired when a later one fails', async () => {
    const manifests = new Map([['x', manifest(['good.png', 'bad.png'])]]);
    await expect(loadTextures(manifests, (_id, file) => `/s/${file}`)).rejects.toMatchObject({
      code: 'asset_load_failed',
    });
    expect(calls.acquire).toEqual(['/s/good.png', '/s/bad.png']);
    expect(calls.release).toEqual(['/s/good.png']);
  });
});
