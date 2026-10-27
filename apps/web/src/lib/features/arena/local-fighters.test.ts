import { describe, expect, it } from 'vitest';
import {
  createLocalAssets,
  loadLocalFighterIds,
  localAssetUrl,
  type FetchLike,
} from './local-fighters.js';

// 002 RF-B7: in dev, a 404 or an invalid answer means "no local fighters", without an error.
function fake(status: number, body: unknown): FetchLike & { urls: string[] } {
  const urls: string[] = [];
  const fn = async (url: string) => {
    urls.push(url);
    return {
      ok: status >= 200 && status < 300,
      json: async () => (body instanceof Error ? Promise.reject(body) : body),
    };
  };
  return Object.assign(fn, { urls });
}

describe('loadLocalFighterIds', () => {
  it('reads the index of the dev server', async () => {
    const fetchFn = fake(200, { ids: ['local_1', 'local_2'] });
    expect(await loadLocalFighterIds(fetchFn)).toEqual(['local_1', 'local_2']);
    expect(fetchFn.urls).toEqual(['/__local-sprites/index.json']);
  });

  it.each([
    ['a 404', fake(404, null)],
    ['invalid JSON', fake(200, new SyntaxError('bad'))],
    ['not { ids }', fake(200, ['local_1'])],
    ['ids that are not strings', fake(200, { ids: [1] })],
    ['an invalid id', fake(200, { ids: ['local_1', '../x'] })],
    ['a placeholder id', fake(200, { ids: ['dummy_a'] })],
  ])('%s gives no local fighters', async (_name, fetchFn) => {
    expect(await loadLocalFighterIds(fetchFn)).toEqual([]);
  });

  it('a failed request gives no local fighters', async () => {
    expect(await loadLocalFighterIds(() => Promise.reject(new TypeError('offline')))).toEqual([]);
  });
});

describe('createLocalAssets', () => {
  it('loads the manifest and resolves the sheets under /__local-sprites/', async () => {
    const fetchFn = fake(200, { id: 'local_1' });
    const assets = createLocalAssets(fetchFn);
    expect(await assets.loadManifest('local_1')).toEqual({ id: 'local_1' });
    expect(fetchFn.urls).toEqual(['/__local-sprites/local_1/manifest.json']);
    expect(assets.sheetUrl('local_1', 'sheet-0.png')).toBe(localAssetUrl('local_1', 'sheet-0.png'));
    expect(localAssetUrl('local_1', 'sheet-0.png')).toBe('/__local-sprites/local_1/sheet-0.png');
  });

  it('a missing manifest rejects with asset_load_failed', async () => {
    await expect(createLocalAssets(fake(404, null)).loadManifest('x')).rejects.toMatchObject({
      code: 'asset_load_failed',
    });
  });
});
