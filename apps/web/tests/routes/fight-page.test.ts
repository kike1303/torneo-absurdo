import type { FightDetail, FightState, FightsCurrent } from '@torneo/protocol';
import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';

vi.mock('$app/env/private', () => ({ INTERNAL_API_URL: 'http://127.0.0.1:8080' }));
const { load } = await import('../../src/routes/fights/[id=fight_id]/+page.server.js');

// 011 RF18, RF23, RF26: the HTML says who fights, and nothing of how it ended.
const VECTORS = JSON.parse(
  readFileSync(
    new URL('../../../server/tests/golden/fight-verify-vectors.json', import.meta.url),
    'utf8',
  ),
) as readonly { name: string; detail: FightDetail }[];
const KO = VECTORS.find((vector) => vector.name === 'ko')?.detail as FightDetail;
const BOARD: FightsCurrent = {
  server_time: '1',
  halted: false,
  current: null,
  next: null,
  last: null,
};

const json = (body: unknown, status = 200) =>
  Promise.resolve(new Response(JSON.stringify(body), { status }));

type Load = (event: {
  fetch: typeof fetch;
  params: { id: string };
  setHeaders: (headers: Record<string, string>) => void;
}) => Promise<Record<string, unknown>>;

function run(
  detail: () => Promise<Response>,
  board: () => Promise<Response> = () => json(BOARD),
  id = '12',
) {
  const urls: string[] = [];
  const headers: Record<string, string>[] = [];
  const fetcher = vi.fn((input: RequestInfo | URL) => {
    const url = String(input);
    urls.push(url);
    return url.endsWith('/current') ? board() : detail();
  });
  const result = (load as unknown as Load)({
    fetch: fetcher as unknown as typeof fetch,
    params: { id },
    setHeaders: (values) => headers.push(values),
  });
  return { result, urls, headers };
}

const inState = (state: FightState): FightDetail => ({
  fight: { ...KO.fight, state, final: null },
  spec_a: KO.spec_a,
  spec_b: KO.spec_b,
  seed: null,
  log: null,
});

describe('the load of /fights/{id}', () => {
  it('a settled fight: only who and when reach the page', async () => {
    const { result, headers, urls } = run(() => json(KO));
    const data = await result;
    expect(data).toEqual({
      status: 'settled',
      id: 12,
      a: KO.fight.a.slug,
      b: KO.fight.b.slug,
      date: KO.fight.live_started_at,
      liveNotice: false,
    });
    expect(headers).toContainEqual({ 'cache-control': 'no-store' });
    expect(urls).toContain('http://127.0.0.1:8080/api/fights/12');
    const text = JSON.stringify(data);
    for (const leak of ['final', 'winner', 'seed', 'log', 'spec_']) {
      expect(text).not.toContain(leak);
    }
    expect(text).not.toContain(KO.seed);
  });

  it.each<FightState>(['scheduled', 'betting_open', 'betting_closed', 'live'])(
    'a fight in %s: "not yet", with who fights and nothing else',
    async (state) => {
      const data = await run(() => json(inState(state))).result;
      expect(data).toEqual({
        status: 'pending',
        id: 12,
        a: KO.fight.a.slug,
        b: KO.fight.b.slug,
        liveNotice: false,
      });
    },
  );

  it('a 404 of the API is a real 404', async () => {
    await expect(run(() => json({ error: 'fight.not_found' }, 404)).result).rejects.toMatchObject({
      status: 404,
    });
  });

  it.each([
    ['a 503', () => json({ error: 'fight.unavailable' }, 503)],
    ['a 429', () => json({ error: 'http.rate_limited' }, 429)],
    ['an API that does not answer', () => Promise.reject(new Error('down'))],
  ])('%s is a state with a retry, never a 500', async (_name, detail) => {
    expect(await run(detail).result).toEqual({ status: 'unavailable', id: 12, liveNotice: false });
  });

  it('a text that is not an id never reaches a read', async () => {
    const { result, urls } = run(() => json(KO), undefined, '../me');
    await expect(result).rejects.toMatchObject({ status: 404 });
    expect(urls).toEqual([]);
  });

  it('notices a live fight, and nothing if the board fails', async () => {
    const live = { ...BOARD, current: { ...KO.fight, state: 'live', final: null } };
    expect(
      (
        await run(
          () => json(KO),
          () => json(live),
        ).result
      ).liveNotice,
    ).toBe(true);
    const down = () => Promise.reject(new Error('down'));
    expect((await run(() => json(KO), down).result).liveNotice).toBe(false);
  });
});
