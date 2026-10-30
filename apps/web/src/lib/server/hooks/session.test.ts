import type { Me } from '@torneo/protocol';
import { describe, expect, it } from 'vitest';
import type { Api } from '#lib/server/api/client.js';
import { createSessionHandle } from './session.js';

// 008 RF-D3, plan-d decision 4.
const PROD = 'https://torneo.example';
const LOOPBACK = 'http://localhost:4173';
const TOKEN = 'c2Vzc2lvbi10b2tlbg';
const ME: Me = {
  display_name: 'Ana',
  role: 'user',
  balance: 100,
  providers: ['dev'],
  locale: 'es',
  theme: 'light',
  display_name_is_generated: false,
};

type Answer = () => Response | Promise<Response>;

interface Deleted {
  name: string;
  options: Record<string, unknown>;
}

function run(
  path: string,
  cookie: string | null,
  answer: Answer,
  publicOrigin = PROD,
  resolved: () => Response = () => new Response('page'),
) {
  const calls: string[] = [];
  const deleted: Deleted[] = [];
  const fetch = (async (input: RequestInfo | URL) => {
    calls.push(String(input));
    return answer();
  }) as typeof globalThis.fetch;
  const handle = createSessionHandle({
    publicOrigin,
    api: (f): Api => ({ fetch: f, base: 'http://torneo-server:8080' }),
  });
  const locals = {} as App.Locals;
  const event = {
    url: new URL(`${publicOrigin}${path}`),
    request: new Request(`${publicOrigin}${path}`, cookie ? { headers: { cookie } } : {}),
    cookies: {
      delete: (name: string, options: Record<string, unknown>) => deleted.push({ name, options }),
    },
    fetch,
    locals,
  };
  const response = Promise.resolve(
    handle({ event: event as never, resolve: () => Promise.resolve(resolved()) }),
  );
  return response.then((res) => ({ res, calls, deleted, locals }));
}

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

describe('createSessionHandle', () => {
  it('without a session cookie never calls the API', async () => {
    const { res, calls, locals } = await run('/es/', 'theme=dark', () => json(200, ME));
    expect(calls).toEqual([]);
    expect(locals.user).toBeNull();
    expect(locals.session).toBe('none');
    expect(res.headers.has('cache-control')).toBe(false);
    expect(res.headers.has('vary')).toBe(false);
  });

  it('200: the user is in locals', async () => {
    const { res, calls, locals } = await run('/es/', `__Host-session=${TOKEN}`, () =>
      json(200, ME),
    );
    expect(calls).toEqual(['http://torneo-server:8080/api/me']);
    expect(locals.user).toEqual(ME);
    expect(locals.session).toBe('valid');
    expect(res.headers.get('cache-control')).toBe('private, no-store');
    expect(res.headers.get('vary')).toBe('Cookie');
  });

  it('401: no user, and the dead cookie is expired with its policy attributes', async () => {
    const { res, locals, deleted } = await run('/es/', `__Host-session=${TOKEN}`, () =>
      json(401, { error: 'auth.unauthenticated' }),
    );
    expect(locals.user).toBeNull();
    expect(locals.session).toBe('none');
    expect(deleted).toEqual([
      {
        name: '__Host-session',
        options: { path: '/', httpOnly: true, sameSite: 'lax', secure: true },
      },
    ]);
    expect(res.headers.get('cache-control')).toBe('private, no-store');
  });

  it('401 on loopback expires ta_dev_session without Secure', async () => {
    const { deleted } = await run(
      '/es/',
      `ta_dev_session=${TOKEN}`,
      () => json(401, { error: 'auth.unauthenticated' }),
      LOOPBACK,
    );
    expect(deleted).toEqual([
      {
        name: 'ta_dev_session',
        options: { path: '/', httpOnly: true, sameSite: 'lax', secure: false },
      },
    ]);
  });

  it.each<[string, Answer]>([
    ['503', () => json(503, { error: 'auth.try_again' })],
    ['429', () => json(429, { error: 'http.rate_limited' })],
    ['500 without JSON', () => new Response('x', { status: 500 })],
    [
      'network error',
      () => {
        throw new TypeError('fetch failed');
      },
    ],
  ])('%s: unavailable, the cookie untouched', async (_name, answer) => {
    const { res, locals, deleted } = await run('/es/account', `__Host-session=${TOKEN}`, answer);
    expect(locals.user).toBeNull();
    expect(locals.session).toBe('unavailable');
    expect(deleted).toEqual([]);
    expect(res.headers.get('cache-control')).toBe('private, no-store');
    expect(res.headers.get('vary')).toBe('Cookie');
  });

  it.each(['/api/me', '/api', '/ws', '/healthz'])(
    '%s is excluded: no call, no headers',
    async (path) => {
      const { res, calls } = await run(path, `__Host-session=${TOKEN}`, () => json(200, ME));
      expect(calls).toEqual([]);
      expect(res.headers.has('cache-control')).toBe(false);
    },
  );

  it('a cookie with the session name but a value the server never issues: no call, still private', async () => {
    const { res, calls, locals } = await run('/es/', '__Host-session="x;y"', () => json(200, ME));
    expect(calls).toEqual([]);
    expect(locals.session).toBe('none');
    expect(res.headers.get('cache-control')).toBe('private, no-store');
  });

  it('replaces any Cache-Control and keeps an existing Vary', async () => {
    const { res } = await run(
      '/es/',
      `__Host-session=${TOKEN}`,
      () => json(200, ME),
      PROD,
      () =>
        new Response('page', {
          headers: { 'cache-control': 'public, max-age=60', vary: 'Accept-Language' },
        }),
    );
    expect(res.headers.get('cache-control')).toBe('private, no-store');
    expect(res.headers.get('vary')).toBe('Accept-Language, Cookie');
  });

  it('works on a response with immutable headers (a redirect)', async () => {
    const { res } = await run(
      '/es/',
      `__Host-session=${TOKEN}`,
      () => json(200, ME),
      PROD,
      () => Response.redirect(`${PROD}/es/`, 303),
    );
    expect(res.status).toBe(303);
    expect(res.headers.get('cache-control')).toBe('private, no-store');
  });
});
