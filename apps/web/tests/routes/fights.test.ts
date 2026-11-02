import type { FightsCurrent, FightsPage, FightSummary } from '@torneo/protocol';
import { describe, expect, it, vi } from 'vitest';
import { match } from '../../src/params/fight_id.js';
import { parseFightId } from '../../src/lib/features/replay/fight-id.js';

vi.mock('$app/env/private', () => ({ INTERNAL_API_URL: 'http://127.0.0.1:8080' }));
const { load } = await import('../../src/routes/fights/+page.server.js');

// 011 RF17, RF18, RF26: the list, without the result, and what reaches a read.
const summary = (id: number, final: FightSummary['final']): FightSummary => ({
  id,
  state: 'settled',
  a: { slug: 'striker', max_hp: 700 },
  b: { slug: 'tank', max_hp: 860 },
  ruleset_version: 1,
  snapshot_sha256: 'ab',
  seed_commit: 'cd',
  opens_at: '1',
  closes_at: '2',
  live_at: '3',
  live_started_at: '4',
  time_limit_ms: 86_000,
  max_duration_ms: 90_000,
  playback_delay_ms: 3500,
  final,
});

const PAGE: FightsPage = {
  fights: [
    summary(9, { kind: 'decided', winner: 'a', reason: 'ko' }),
    { ...summary(8, { kind: 'cancelled', reason: 'missed_window' }), live_started_at: null },
    summary(7, { kind: 'voided', reason: 'engine_error' }),
    summary(6, { kind: 'decided', winner: null, reason: 'draw' }),
    summary(5, { kind: 'a_new_kind' } as unknown as FightSummary['final']),
  ],
  next_before: 5,
};
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
  url: URL;
  setHeaders: (headers: Record<string, string>) => void;
}) => Promise<{
  rows: { kind: string | null; date: string; id: number }[] | null;
  nextBefore: number | null;
  liveNotice: boolean;
}>;

function run(
  query: string,
  page: () => Promise<Response> = () => json(PAGE),
  board: () => Promise<Response> = () => json(BOARD),
) {
  const urls: string[] = [];
  const headers: Record<string, string>[] = [];
  const fetcher = vi.fn((input: RequestInfo | URL) => {
    const url = String(input);
    urls.push(url);
    return url.endsWith('/api/fights/current') ? board() : page();
  });
  const result = (load as unknown as Load)({
    fetch: fetcher as unknown as typeof fetch,
    url: new URL(`http://localhost/es/fights${query}`),
    setHeaders: (values) => headers.push(values),
  });
  return { result, urls, headers };
}

describe('the load of /fights', () => {
  it('gives who, when and the kind of ending, and no winner', async () => {
    const { result, headers } = run('');
    const data = await result;
    expect(data.rows?.map((row) => [row.id, row.kind])).toEqual([
      [9, 'played'],
      [8, 'cancelled'],
      [7, 'voided'],
      [6, 'played'],
      [5, null],
    ]);
    expect(data.rows?.[0]?.date).toBe('4');
    expect(data.rows?.[1]?.date).toBe('3');
    expect(data.nextBefore).toBe(5);
    expect(headers).toContainEqual({ 'cache-control': 'no-store' });
    const text = JSON.stringify(data);
    for (const leak of ['winner', 'reason', 'final', '"ko"', 'draw', 'seed', 'log']) {
      expect(text).not.toContain(leak);
    }
  });

  it('passes a canonical before', async () => {
    const { result, urls } = run('?before=12');
    await result;
    expect(urls).toContain('http://127.0.0.1:8080/api/fights?before=12');
  });

  it.each(['?before=01', '?before=abc', '?before=-1', '?before=12%26x%3D1', '?before=2147483648'])(
    '%s gives the first page',
    async (query) => {
      const { result, urls } = run(query);
      await result;
      expect(urls).toContain('http://127.0.0.1:8080/api/fights');
    },
  );

  it.each([
    ['a 503', () => json({ error: 'fight.unavailable' }, 503)],
    ['a 429', () => json({ error: 'http.rate_limited' }, 429)],
    ['an API that does not answer', () => Promise.reject(new Error('down'))],
  ])('%s is the error state, without throwing', async (_name, page) => {
    expect((await run('', page).result).rows).toBeNull();
  });

  it('notices a live fight, and nothing if the board fails or nothing is live', async () => {
    const live = { ...BOARD, current: { ...summary(1, null), state: 'live' } };
    const open = { ...BOARD, current: { ...summary(1, null), state: 'betting_open' } };
    expect((await run('', undefined, () => json(live)).result).liveNotice).toBe(true);
    expect((await run('', undefined, () => json(open)).result).liveNotice).toBe(false);
    expect((await run('').result).liveNotice).toBe(false);
    const down = () => Promise.reject(new Error('down'));
    expect((await run('', undefined, down).result).liveNotice).toBe(false);
  });
});

describe('the fight_id matcher', () => {
  it.each(['1', '12', '2147483647'])('accepts %s', (text) => {
    expect(match(text)).toBe(true);
  });

  // Kit decodes the path: `..%2Fme` arrives here as `../me`.
  it.each([
    '',
    '0',
    '01',
    '+1',
    '-1',
    ' 1',
    '1 ',
    '1.0',
    '0x1',
    '12?x',
    '../me',
    '..',
    '../auth/google/callback?code=x',
    '2147483648',
    '99999999999999999999',
    '١',
  ])('rejects %j', (text) => {
    expect(match(text)).toBe(false);
    expect(parseFightId(text)).toBeNull();
  });
});
