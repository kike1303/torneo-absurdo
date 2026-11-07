import type { FightSummary, FightsCurrent, VoidStats } from '@torneo/protocol';
import { beforeEach, describe, expect, it, vi } from 'vitest';

// 010 RF53 and plan-d decision 14: a failure is a 200 with the error state, and `debug` only exists
// next to the dev catalog.
const env = vi.hoisted(() => ({ ENABLE_DEV_CATALOG: '' as string | undefined }));
vi.mock('$app/env/private', () => ({
  get ENABLE_DEV_CATALOG() {
    return env.ENABLE_DEV_CATALOG;
  },
  INTERNAL_API_URL: 'http://127.0.0.1:8080',
}));
const { load } = await import('../../src/routes/+page.server.js');

const BOARD: FightsCurrent = {
  server_time: '1000',
  halted: false,
  current: null,
  next: null,
  last: null,
};

type Load = (event: {
  fetch: typeof fetch;
  url: URL;
  setHeaders: (headers: Record<string, string>) => void;
}) => Promise<{
  board: FightsCurrent | null;
  voidStats: VoidStats | null;
  debug: boolean;
}>;

const headers: Record<string, string>[] = [];

function run(respond: () => Promise<Response>, query = '') {
  return (load as unknown as Load)({
    fetch: vi.fn(respond) as unknown as typeof fetch,
    url: new URL(`http://localhost/es/${query}`),
    setHeaders: (values) => headers.push(values),
  });
}

const json = (body: unknown, status = 200) =>
  Promise.resolve(
    new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    }),
  );

beforeEach(() => {
  env.ENABLE_DEV_CATALOG = undefined;
});

describe('the load of the home page', () => {
  it('gives the board', async () => {
    expect(await run(() => json(BOARD))).toEqual({
      board: BOARD,
      voidStats: null,
      debug: false,
    });
  });

  it('a 503 of the API is the error state, without throwing', async () => {
    expect(await run(() => json({ error: 'fight.unavailable' }, 503))).toEqual({
      board: null,
      voidStats: null,
      debug: false,
    });
  });

  it('an API that does not answer is the error state, without throwing', async () => {
    const result = await run(() => Promise.reject(new Error('down')));
    expect(result).toEqual({ board: null, voidStats: null, debug: false });
  });

  // 012 RF22: the board comes from memory, on purpose. The counter touches the database, so only a
  // last fight that needs it reads it, and its failure only leaves out one line.
  const lastWith = (final: FightSummary['final']): FightsCurrent => ({
    ...BOARD,
    last: { final } as FightSummary,
  });
  const reads = (board: FightsCurrent, counter: () => Promise<Response>) =>
    vi.fn((input: RequestInfo | URL) =>
      String(input).endsWith('/void-stats') ? counter() : json(board),
    );
  const call = (fetchFn: unknown) =>
    (load as unknown as Load)({
      fetch: fetchFn as typeof fetch,
      url: new URL('http://localhost/es/'),
      setHeaders: () => {},
    });

  it('reads the counter only for a last fight voided with its secret lost', async () => {
    const lost = lastWith({ kind: 'voided', reason: 'secret_lost' });
    const stats = { voided: 4, secret_lost: 1 };
    const fetchFn = reads(lost, () => json(stats));
    expect((await call(fetchFn)).voidStats).toEqual(stats);
    expect(fetchFn).toHaveBeenCalledTimes(2);

    for (const board of [
      BOARD,
      lastWith({ kind: 'voided', reason: 'engine_error' }),
      lastWith({ kind: 'cancelled', reason: 'missed_window' }),
    ]) {
      const other = reads(board, () => json(stats));
      expect((await call(other)).voidStats).toBeNull();
      expect(other).toHaveBeenCalledTimes(1);
    }
  });

  it('a counter that fails does not take the board with it', async () => {
    const lost = lastWith({ kind: 'voided', reason: 'secret_lost' });
    const result = await call(reads(lost, () => json({ error: 'fight.unavailable' }, 503)));
    expect(result).toEqual({ board: lost, voidStats: null, debug: false });
  });

  it('is never cached: a stored board would show an old fight as the current one', async () => {
    headers.length = 0;
    await run(() => json(BOARD));
    expect(headers).toEqual([{ 'cache-control': 'no-store' }]);
  });

  it('calls the API with a read', async () => {
    const respond = vi.fn(() => json(BOARD));
    await (load as unknown as Load)({
      fetch: respond as unknown as typeof fetch,
      url: new URL('http://localhost/es/'),
      setHeaders: () => undefined,
    });
    const [url, init] = respond.mock.calls[0] as unknown as [string, RequestInit];
    expect(url).toBe('http://127.0.0.1:8080/api/fights/current');
    expect(init.method).toBe('GET');
  });

  it.each([
    [undefined, '?debug=1', false],
    ['0', '?debug=1', false],
    ['1', '', false],
    ['1', '?debug=0', false],
    ['1', '?debug=1', true],
  ])('debug with ENABLE_DEV_CATALOG=%s and %s is %s', async (catalog, query, debug) => {
    env.ENABLE_DEV_CATALOG = catalog;
    expect((await run(() => json(BOARD), query)).debug).toBe(debug);
  });
});
