import type { FightsCurrent } from '@torneo/protocol';
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

type Load = (event: { fetch: typeof fetch; url: URL }) => Promise<{
  board: FightsCurrent | null;
  debug: boolean;
}>;

function run(respond: () => Promise<Response>, query = '') {
  return (load as unknown as Load)({
    fetch: vi.fn(respond) as unknown as typeof fetch,
    url: new URL(`http://localhost/es/${query}`),
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
    expect(await run(() => json(BOARD))).toEqual({ board: BOARD, debug: false });
  });

  it('a 503 of the API is the error state, without throwing', async () => {
    expect(await run(() => json({ error: 'fight.unavailable' }, 503))).toEqual({
      board: null,
      debug: false,
    });
  });

  it('an API that does not answer is the error state, without throwing', async () => {
    const result = await run(() => Promise.reject(new Error('down')));
    expect(result).toEqual({ board: null, debug: false });
  });

  it('calls the API with a read', async () => {
    const respond = vi.fn(() => json(BOARD));
    await (load as unknown as Load)({
      fetch: respond as unknown as typeof fetch,
      url: new URL('http://localhost/es/'),
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
