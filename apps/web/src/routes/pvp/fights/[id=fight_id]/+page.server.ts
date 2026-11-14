import { error } from '@sveltejs/kit';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { internalApi } from '#lib/server/api/context.js';
import { getPvpFightDetail } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The replay of a player fight (021 "Web"), public. The SSR gives real 404s and says who fought,
// never the result: the log, the seed and the specs are read by the browser itself, so it checks
// what came straight from the server (011).
export const load: PageServerLoad = async ({ fetch, params, setHeaders }) => {
  setHeaders({ 'cache-control': 'no-store' });
  const id = parseFightId(params.id);
  if (id === null) error(404);
  const result = await getPvpFightDetail(internalApi(fetch), id);
  if (!result.ok) {
    if ('status' in result && result.status === 404) error(404);
    return { status: 'unavailable' as const, id };
  }
  return { status: 'ready' as const, id, a: result.data.a, b: result.data.b };
};
