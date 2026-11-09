import type { Leaderboard, SeasonView } from '@torneo/protocol';
import { describe, expect, it, vi } from 'vitest';

vi.mock('$app/env/private', () => ({ INTERNAL_API_URL: 'http://internal.test' }));
const { load } = await import('../../src/routes/leaderboard/+page.server.js');

// 020: the load of /leaderboard. Public, `season` only as a number, and a failure of the API as the
// error state.
const SEASON: SeasonView = { id: 202611, starts_at: '1', ends_at: '2', closed: false };
const BOARD: Leaderboard = {
  season: SEASON,
  entries: [{ rank: 1, display_name: 'Ana', ranked_profit: 40, settled_fights: 12 }],
  min_fights: 10,
};

type Load = (event: {
  fetch: typeof fetch;
  url: URL;
  setHeaders: (headers: Record<string, string>) => void;
}) => Promise<{ board: Leaderboard | null; seasons: SeasonView[] }>;

function run(query: string, board: () => Response = () => Response.json(BOARD)) {
  const urls: string[] = [];
  const headers: Record<string, string>[] = [];
  const fetcher = vi.fn(async (input: RequestInfo | URL) => {
    const url = new URL(String(input));
    urls.push(url.pathname + url.search);
    return url.pathname === '/api/seasons' ? Response.json([SEASON]) : board();
  });
  const result = (load as unknown as Load)({
    fetch: fetcher as unknown as typeof fetch,
    url: new URL(`http://localhost/es/leaderboard${query}`),
    setHeaders: (values) => headers.push(values),
  });
  return { result, urls, headers };
}

describe('/leaderboard load (020)', () => {
  it('reads the board of this moment and the seasons, with no-store', async () => {
    const { result, urls, headers } = run('');
    expect(await result).toEqual({ board: BOARD, seasons: [SEASON] });
    expect(urls.sort()).toEqual(['/api/leaderboard', '/api/seasons']);
    expect(headers).toEqual([{ 'cache-control': 'no-store' }]);
  });

  it.each([
    ['?season=202610', '/api/leaderboard?season=202610'],
    ['?season=canary-x', '/api/leaderboard'],
    ['?season=202613', '/api/leaderboard'],
    ['?season=2026101', '/api/leaderboard'],
  ])('%s reads %s', async (query, expected) => {
    const { result, urls } = run(query);
    await result;
    expect(urls).toContain(expected);
  });

  it('a failure of the API is the error state, not a 500', async () => {
    const { result } = run('', () => new Response('', { status: 503 }));
    expect(await result).toEqual({ board: null, seasons: [SEASON] });
  });
});
