import { parseSeasonId } from '#lib/features/seasons/season-id.js';
import { internalApi } from '#lib/server/api/context.js';
import { getLeaderboard, getSeasons } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The leaderboard of bettors (020), without a session. `season` reaches the read only as a number;
// one that is not a season is the season of this moment. A failure of the API is a 200 with the
// error state, never a 500.
export const load: PageServerLoad = async ({ fetch, url, setHeaders }) => {
  setHeaders({ 'cache-control': 'no-store' });
  const season = parseSeasonId(url.searchParams.get('season') ?? '');
  const api = internalApi(fetch);
  const [board, seasons] = await Promise.all([getLeaderboard(api, season), getSeasons(api)]);
  return {
    board: board.ok ? board.data : null,
    // A season well written but without a row: its own text, not the error of a failed load.
    notFound: !board.ok && 'status' in board && board.status === 404,
    seasons: seasons.ok && Array.isArray(seasons.data) ? seasons.data : [],
  };
};
