import { ENABLE_DEV_CATALOG } from '$app/env/private';
import { internalApi } from '#lib/server/api/context.js';
import { getFightsCurrent } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The home page (010 RF53): what is on now, without a session. A failure is the error state of the
// view, with a 200, never a 500: the client still tries by itself. `debug` only exists next to the
// dev catalog, so production never gets those attributes (plan-d decision 14).
export const load: PageServerLoad = async ({ fetch, url }) => {
  const result = await getFightsCurrent(internalApi(fetch));
  return {
    board: result.ok ? result.data : null,
    debug: ENABLE_DEV_CATALOG === '1' && url.searchParams.get('debug') === '1',
  };
};
