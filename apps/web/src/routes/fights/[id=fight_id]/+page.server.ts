import { error } from '@sveltejs/kit';
import { fightDate } from '#lib/features/replay/fight-row.js';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { internalApi } from '#lib/server/api/context.js';
import { getFightDetail, getFightsCurrent } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The page of a fight (011 RF18, RF23). The SSR gives real 404s and says who fights: the final, the
// log, the seed and the specs never go in the HTML. The browser reads them by itself, so it checks
// what came straight from the server.
export const load: PageServerLoad = async ({ fetch, params, setHeaders }) => {
  setHeaders({ 'cache-control': 'no-store' });
  // The matcher already let only this through; a read never takes a text of the URL.
  const id = parseFightId(params.id);
  if (id === null) error(404);
  const api = internalApi(fetch);
  const [result, board] = await Promise.all([getFightDetail(api, id), getFightsCurrent(api)]);
  // Read once, with no polling: it may go stale, and its link stays valid (RF26).
  const liveNotice = board.ok && board.data.current?.state === 'live';
  if (!result.ok) {
    if ('status' in result && result.status === 404) error(404);
    return { status: 'unavailable' as const, id, liveNotice };
  }
  const { fight } = result.data;
  const crossing = { id, a: fight.a.slug, b: fight.b.slug, liveNotice };
  return fight.state === 'settled'
    ? { status: 'settled' as const, ...crossing, date: fightDate(fight) }
    : { status: 'pending' as const, ...crossing };
};
