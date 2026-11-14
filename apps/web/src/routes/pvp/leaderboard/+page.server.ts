import { parseSeasonId } from '#lib/features/seasons/season-id.js';
import { internalApi } from '#lib/server/api/context.js';
import { getPvpLeaderboard, getPvpSeasons } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The PvP leaderboard (021), without a session. `season` reaches the read only as a number; one
// that is not a season is the current one. A failure of the API is a 200 with the error state.
export const load: PageServerLoad = async ({ fetch, url, setHeaders }) => {
  setHeaders({ 'cache-control': 'no-store' });
  const season = parseSeasonId(url.searchParams.get('season') ?? '');
  const api = internalApi(fetch);
  const [board, seasons] = await Promise.all([getPvpLeaderboard(api, season), getPvpSeasons(api)]);
  return {
    board: board.ok ? board.data : null,
    notFound: !board.ok && 'status' in board && board.status === 404,
    seasons: seasons.ok && Array.isArray(seasons.data) ? seasons.data : [],
  };
};
