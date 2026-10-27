// Fighters converted locally (002 RF-B7, plan-b decisions 14 and 15). Everything sits behind
// `import.meta.env.DEV`: the build replaces it with `false` and drops the requests and the
// `/__local-sprites/` URLs (scripts/check-no-local-sprites.mjs checks build/). In `vite dev`, a
// 404 or an invalid answer means "no local fighters", without an error.
import { ARENA_FIGHTERS } from './fighters.js';

const ID = /^[a-z][a-z0-9_]{0,47}$/;
const MAX_IDS = 100;

export type FetchLike = (url: string) => Promise<Pick<Response, 'ok' | 'json'>>;

/** How FightStage loads a fighter that is not a placeholder (passed by props, ADR-0009 D4). */
export interface LocalAssets {
  loadManifest(id: string): Promise<unknown>;
  sheetUrl(id: string, file: string): string;
}

function isLocalId(value: unknown): value is string {
  return (
    typeof value === 'string' &&
    ID.test(value) &&
    !(ARENA_FIGHTERS as readonly string[]).includes(value)
  );
}

export async function loadLocalFighterIds(fetchFn: FetchLike): Promise<string[]> {
  if (import.meta.env.DEV) {
    const res = await fetchFn('/__local-sprites/index.json').catch(() => null);
    if (res === null || !res.ok) return [];
    const body: unknown = await res.json().catch(() => null);
    const ids = (body as { ids?: unknown } | null)?.ids;
    if (!Array.isArray(ids) || !ids.every(isLocalId)) return [];
    return [...new Set(ids)].slice(0, MAX_IDS);
  }
  return [];
}

export function localAssetUrl(id: string, file: string): string {
  if (import.meta.env.DEV) return `/__local-sprites/${id}/${file}`;
  return '';
}

export function createLocalAssets(fetchFn: FetchLike): LocalAssets {
  return {
    async loadManifest(id) {
      if (import.meta.env.DEV) {
        const res = await fetchFn(localAssetUrl(id, 'manifest.json'));
        if (!res.ok)
          throw Object.assign(new Error('asset_load_failed'), { code: 'asset_load_failed' });
        return res.json();
      }
      throw Object.assign(new Error('asset_load_failed'), { code: 'asset_load_failed' });
    },
    sheetUrl: localAssetUrl,
  };
}
