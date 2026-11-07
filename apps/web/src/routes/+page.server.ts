import { ENABLE_DEV_CATALOG } from '$app/env/private';
import { internalApi } from '#lib/server/api/context.js';
import { getFightsCurrent, getVoidStats } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The home page (010 RF53): what is on now, without a session. A failure is the error state of the
// view, with a 200, never a 500: the client still tries by itself. `debug` only exists next to the
// dev catalog, so production never gets those attributes (plan-d decision 14).
export const load: PageServerLoad = async ({ fetch, url, setHeaders }) => {
  // What is on now: a cached board would show an old fight as the current one.
  setHeaders({ 'cache-control': 'no-store' });
  const api = internalApi(fetch);
  const result = await getFightsCurrent(api);
  const board = result.ok ? result.data : null;
  // The board comes from memory, on purpose. The counter touches the database: only when the last
  // fight needs it (012 RF22). Its failure only leaves out one line.
  const last = board?.last?.final ?? null;
  const voidStats =
    last?.kind === 'voided' && last.reason === 'secret_lost' ? await getVoidStats(api) : null;
  return {
    board,
    voidStats: voidStats?.ok ? voidStats.data : null,
    debug: ENABLE_DEV_CATALOG === '1' && url.searchParams.get('debug') === '1',
  };
};
