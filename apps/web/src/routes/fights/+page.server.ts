import { fightRow } from '#lib/features/replay/fight-row.js';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { internalApi } from '#lib/server/api/context.js';
import { getFightsCurrent, getFightsPage } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The past fights (011 RF17), without a session. A failure of the API is a 200 with the error state,
// never a 500. Only `fightRow` reaches the page: nothing of how a fight ended but its kind (RF20).
export const load: PageServerLoad = async ({ fetch, url, setHeaders }) => {
  setHeaders({ 'cache-control': 'no-store' });
  const before = parseFightId(url.searchParams.get('before') ?? '');
  const api = internalApi(fetch);
  const [page, board] = await Promise.all([getFightsPage(api, before), getFightsCurrent(api)]);
  return {
    rows: page.ok ? page.data.fights.map(fightRow) : null,
    nextBefore: page.ok ? page.data.next_before : null,
    // Read once, with no polling: it may go stale, and its link stays valid (RF26).
    liveNotice: board.ok && board.data.current?.state === 'live',
  };
};
