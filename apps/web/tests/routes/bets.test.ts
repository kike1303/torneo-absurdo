import type { BetHistoryPage, Me } from '@torneo/protocol';
import { describe, expect, it, vi } from 'vitest';
import { fakeEvent, redirectOf } from '../support/event.js';

vi.mock('$app/env/private', () => ({ INTERNAL_API_URL: 'http://internal.test' }));
const { load } = await import('../../src/routes/bets/+page.server.js');

// 019: the load of /bets. Only with a session, a fixed `next`, `before` only as a number, and a
// failure of the API as the error state.
const ORIGIN = 'http://localhost:4173';
const CANARY = 'canary-history-5e1b';
const me: Me = {
  display_name: 'Ana',
  role: 'user',
  balance: 100,
  providers: ['dev'],
  locale: null,
  theme: null,
  display_name_is_generated: false,
};
const PAGE: BetHistoryPage = {
  items: [
    {
      fight_id: 9,
      a: 'striker',
      b: 'tank',
      side: 'b',
      staked: 40,
      bets: 1,
      placed_at: '1000',
      settled: true,
      result: { outcome: 'lost', payout: 0, net: -40, refund_reason: null },
    },
  ],
  next_before: 9,
};

type Handler = (event: never) => unknown;
const callLoad = load as unknown as Handler;

function run(
  path: string,
  init: { user?: Me | null; session?: App.Locals['session']; answer?: () => Response } = {},
) {
  const urls: string[] = [];
  const headers: Record<string, string>[] = [];
  const user = init.user === undefined ? me : init.user;
  const event = fakeEvent({
    url: `${ORIGIN}${path}`,
    fetch: (async (input: string | URL | Request) => {
      urls.push(String(input));
      return (init.answer ?? (() => new Response(JSON.stringify(PAGE), { status: 200 })))();
    }) as typeof globalThis.fetch,
    locals: { user, session: init.session ?? (user ? 'valid' : 'none') },
  });
  Object.assign(event, { setHeaders: (values: Record<string, string>) => headers.push(values) });
  return {
    result: () => callLoad(event as never) as Promise<Record<string, unknown>>,
    urls,
    headers,
  };
}

describe('/bets load (019)', () => {
  it('without a session goes to the login, with nothing of the query', async () => {
    const { result, urls } = run(`/es/bets?before=${CANARY}`, { user: null });
    const { status, location } = await redirectOf(result);
    expect(status).toBe(303);
    expect(location).toBe('/es/login?next=%2Fes%2Fbets');
    expect(urls).toEqual([]);
  });

  it('with the session down, says so and reads nothing', async () => {
    const { result, urls } = run('/en/bets', { user: null, session: 'unavailable' });
    expect(await result()).toEqual({ state: 'unavailable' });
    expect(urls).toEqual([]);
  });

  it('reads the first page and turns it into rows', async () => {
    const { result, urls, headers } = run('/es/bets');
    const data = await result();
    expect(data).toMatchObject({ state: 'ready', nextBefore: 9 });
    expect(data.rows).toEqual([expect.objectContaining({ id: 9, status: 'lost', net: -40 })]);
    expect(urls.map((url) => new URL(url).pathname + new URL(url).search)).toEqual([
      '/api/bets/history',
    ]);
    expect(headers).toEqual([{ 'cache-control': 'private, no-store' }]);
  });

  it.each([
    ['?before=7', '/api/bets/history?before=7'],
    [`?before=${CANARY}`, '/api/bets/history'],
    ['?before=07', '/api/bets/history'],
    ['?before=2147483648', '/api/bets/history'],
  ])('%s reads %s', async (query, expected) => {
    const { result, urls } = run(`/es/bets${query}`);
    await result();
    const url = new URL(urls[0] ?? '');
    expect(url.pathname + url.search).toBe(expected);
  });

  it('a failure of the API is the error state, not a 500', async () => {
    const { result } = run('/es/bets', { answer: () => new Response('', { status: 503 }) });
    expect(await result()).toEqual({ state: 'ready', rows: null, nextBefore: null });
  });
});
