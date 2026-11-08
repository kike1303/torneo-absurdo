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
const { actions, load } = await import('../../src/routes/+page.server.js');

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
  locals: { user: unknown; session: string };
}) => Promise<{
  board: FightsCurrent | null;
  voidStats: VoidStats | null;
  debug: boolean;
  betting: unknown;
}>;

/** What the panel gets without a session (012 RF42): nothing read, and a fresh key. */
const SIGNED_OUT = {
  signedIn: false,
  wallet: null,
  mine: {},
  pool: null,
  target: null,
  clientKey: expect.stringMatching(
    /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
  ),
};

const headers: Record<string, string>[] = [];

function run(respond: () => Promise<Response>, query = '') {
  return (load as unknown as Load)({
    fetch: vi.fn(respond) as unknown as typeof fetch,
    url: new URL(`http://localhost/es/${query}`),
    setHeaders: (values) => headers.push(values),
    locals: { user: null, session: 'none' },
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
      betting: SIGNED_OUT,
    });
  });

  it('a 503 of the API is the error state, without throwing', async () => {
    expect(await run(() => json({ error: 'fight.unavailable' }, 503))).toEqual({
      board: null,
      voidStats: null,
      debug: false,
      betting: SIGNED_OUT,
    });
  });

  it('an API that does not answer is the error state, without throwing', async () => {
    const result = await run(() => Promise.reject(new Error('down')));
    expect(result).toEqual({ board: null, voidStats: null, debug: false, betting: SIGNED_OUT });
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
      locals: { user: null, session: 'none' },
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
    expect(result).toEqual({ board: lost, voidStats: null, debug: false, betting: SIGNED_OUT });
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
      locals: { user: null, session: 'none' },
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

// 012 RF44 and plan-c C1: with a session, the wallet and the bets of up to three fights, and the pool
// of the target; a failed read leaves only its value out.
describe('the load of the home page with a session', () => {
  const fight = (id: number, state: FightSummary['state']) => ({ id, state }) as FightSummary;
  const board: FightsCurrent = {
    ...BOARD,
    current: fight(2, 'live'),
    next: fight(3, 'betting_open'),
    last: fight(1, 'settled'),
  };
  const mine = (id: number) => ({
    fight_id: id,
    side: null,
    staked: 0,
    max_stake: 100,
    bets: [],
    result: null,
  });

  function answer(input: RequestInfo | URL): Promise<Response> {
    const path = new URL(String(input)).pathname + new URL(String(input)).search;
    if (path === '/api/fights/current') return json(board);
    if (path === '/api/wallet') return json({ balance: 100 });
    if (path === '/api/fights/3/pool') return json({ fight_id: 3 });
    const id = /fight_id=(\d+)/.exec(path)?.[1];
    if (id === '2') return json({ error: 'bet.unavailable' }, 503);
    if (id !== undefined) return json(mine(Number(id)));
    return json({ error: 'http.not_found' }, 404);
  }

  it('reads what the panel needs, and leaves out only what failed', async () => {
    const fetchFn = vi.fn(answer);
    const result = await (load as unknown as Load)({
      fetch: fetchFn as unknown as typeof fetch,
      url: new URL('http://localhost/es/'),
      setHeaders: () => undefined,
      locals: { user: { id: 'u' }, session: 'active' },
    });
    expect(result.betting).toEqual({
      signedIn: true,
      wallet: { balance: 100 },
      mine: { 1: mine(1), 3: mine(3) },
      pool: { fight_id: 3 },
      target: 3,
      clientKey: expect.any(String),
    });
    const paths = fetchFn.mock.calls.map(([input]) => new URL(String(input)).pathname);
    expect(paths.filter((path) => path === '/api/bets/mine')).toHaveLength(3);
  });
});

type Action = (event: {
  request: Request;
  url: URL;
  locals: { user: unknown; session: string };
  fetch: typeof fetch;
}) => Promise<unknown>;

const KEY = '0f8fad5b-d9cb-469f-a165-70867728950e';

function post(fields: Record<string, string>, user: unknown, respond: () => Promise<Response>) {
  const body = new FormData();
  for (const [name, value] of Object.entries(fields)) body.set(name, value);
  const fetchFn = vi.fn(respond);
  return {
    fetchFn,
    run: (action: Action) =>
      action({
        request: new Request('http://localhost/es/?/bet', { method: 'POST', body }),
        url: new URL('http://localhost/es/?/bet'),
        locals: { user, session: user === null ? 'none' : 'active' },
        fetch: fetchFn as unknown as typeof fetch,
      }),
  };
}

describe('the actions of the panel', () => {
  const bet = actions.bet as unknown as Action;
  const fields = { fight_id: '3', side: 'a', amount: '40', client_key: KEY };

  it('a bet goes to the API as it was typed', async () => {
    const { fetchFn, run: send } = post(fields, { id: 'u' }, () => json({}, 201));
    expect(await send(bet)).toEqual({ placed: true });
    const [, init] = fetchFn.mock.calls[0] as unknown as [string, RequestInit];
    expect(JSON.parse(String(init.body))).toEqual({
      fight_id: 3,
      side: 'a',
      amount: 40,
      client_key: KEY,
    });
  });

  it('a refusal keeps the code, the amount and the side', async () => {
    const { run: send } = post(fields, { id: 'u' }, () => json({ error: 'bet.side_locked' }, 409));
    expect(await send(bet)).toMatchObject({
      status: 409,
      data: { code: 'bet.side_locked', amount: '40', side: 'a', fight: 3 },
    });
  });

  it('a form of the wrong shape never reaches the API', async () => {
    const { fetchFn, run: send } = post({ ...fields, amount: '1.5' }, { id: 'u' }, () => json({}));
    expect(await send(bet)).toMatchObject({
      status: 422,
      data: { code: 'bet.invalid_request', amount: '1.5' },
    });
    expect(fetchFn).not.toHaveBeenCalled();
  });

  it('an API that does not answer is bet.unavailable, so the panel offers a retry', async () => {
    const { run: send } = post(fields, { id: 'u' }, () => Promise.reject(new Error('down')));
    expect(await send(bet)).toMatchObject({ status: 503, data: { code: 'bet.unavailable' } });
  });

  it('without a session it goes to the login, and back to the home page', async () => {
    const { run: send } = post(fields, null, () => json({}));
    await expect(send(bet)).rejects.toMatchObject({
      status: 303,
      location: `/es/login?next=${encodeURIComponent('/es/')}`,
    });
  });

  it('the claims answer with what was claimed, or with the code of the refusal', async () => {
    const bonus = actions.claimBonus as unknown as Action;
    const bailout = actions.claimBailout as unknown as Action;
    expect(await post({}, { id: 'u' }, () => json({ balance: 600 })).run(bonus)).toEqual({
      claimed: 'bonus',
    });
    expect(
      await post({}, { id: 'u' }, () => json({ error: 'bailout.not_eligible' }, 409)).run(bailout),
    ).toMatchObject({ status: 409, data: { code: 'bailout.not_eligible' } });
  });
});
