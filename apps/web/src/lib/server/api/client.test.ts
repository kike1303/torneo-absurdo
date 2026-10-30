import { describe, expect, it } from 'vitest';
import { apiRequest, type Api } from './client.js';
import { getMe, getProviders } from './reads.js';
import { postUnlink, putDisplayName, putPreferences } from './mutations.js';

// 008 plan-d decision 3.
const BASE = 'http://torneo-server:8080';
const CANARY = 'canary-body-5e1d';

function api(
  answer: (request: Request, signal?: AbortSignal | null) => Response | Promise<Response>,
): {
  api: Api;
  seen: Request[];
} {
  const seen: Request[] = [];
  const fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = new Request(input, init);
    seen.push(request);
    return answer(request, init?.signal);
  }) as typeof globalThis.fetch;
  return { api: { fetch, base: BASE }, seen };
}

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status, headers: { 'content-type': 'application/json' } });

describe('apiRequest', () => {
  it('returns the data of a 200', async () => {
    const { api: a, seen } = api(() => json(200, { x: 1 }));
    expect(await apiRequest(a, 'GET', '/api/me')).toEqual({
      ok: true,
      status: 200,
      data: { x: 1 },
    });
    expect(seen[0]?.url).toBe(`${BASE}/api/me`);
    expect(seen[0]?.method).toBe('GET');
  });

  it('treats a 303 as success, without following it', async () => {
    const { api: a } = api(() => new Response(null, { status: 303, headers: { location: '/' } }));
    expect(await apiRequest(a, 'POST', '/api/auth/unlink/google')).toEqual({
      ok: true,
      status: 303,
      data: undefined,
    });
  });

  it.each([
    [409, { error: 'auth.last_provider' }, 'auth.last_provider'],
    [422, { error: 'profile.display_name_taken' }, 'profile.display_name_taken'],
    [429, { error: 'http.rate_limited' }, 'http.rate_limited'],
    [401, { error: 'auth.unauthenticated' }, 'auth.unauthenticated'],
    // A code the protocol does not know, or no code at all, is `undefined`.
    [409, { error: CANARY }, undefined],
    [409, { error: '__proto__' }, undefined],
    [409, { error: 'toString' }, undefined],
    [500, { nope: 1 }, undefined],
    [500, [CANARY], undefined],
  ])('%i %j → code %j', async (status, body, code) => {
    const { api: a } = api(() => json(status, body));
    expect(await apiRequest(a, 'GET', '/api/me')).toEqual({ ok: false, status, code });
  });

  it('an error body that is not JSON gives no code, and the body never surfaces', async () => {
    const { api: a } = api(() => new Response(`<html>${CANARY}</html>`, { status: 502 }));
    const result = await apiRequest(a, 'GET', '/api/me');
    expect(result).toEqual({ ok: false, status: 502, code: undefined });
    expect(JSON.stringify(result)).not.toContain(CANARY);
  });

  it('a 200 that is not JSON is "unavailable", without the body', async () => {
    const { api: a } = api(() => new Response(CANARY, { status: 200 }));
    const result = await apiRequest(a, 'GET', '/api/me');
    expect(result).toEqual({ ok: false, unavailable: true });
  });

  it('a network error is "unavailable" and never throws', async () => {
    const { api: a } = api(() => {
      throw new TypeError(`fetch failed ${CANARY}`);
    });
    const result = await apiRequest(a, 'GET', '/api/me');
    expect(result).toEqual({ ok: false, unavailable: true });
  });

  it('a slow server is "unavailable" after the timeout', async () => {
    const { api: a } = api(
      (_request, signal) =>
        new Promise<Response>((_, reject) => {
          signal?.addEventListener('abort', () => reject(signal.reason));
        }),
    );
    const started = Date.now();
    expect(await apiRequest(a, 'GET', '/api/me', { timeoutMs: 50 })).toEqual({
      ok: false,
      unavailable: true,
    });
    expect(Date.now() - started).toBeLessThan(1000);
  });

  it('sends JSON bodies with their content type', async () => {
    const { api: a, seen } = api(() => json(200, {}));
    await apiRequest(a, 'PUT', '/api/me/preferences', { body: { theme: 'light' } });
    expect(seen[0]?.headers.get('content-type')).toBe('application/json');
    expect(await seen[0]?.text()).toBe('{"theme":"light"}');
  });
});

describe('reads use GET only, mutations their own method', () => {
  it('getMe and getProviders', async () => {
    const { api: a, seen } = api(() => json(200, []));
    await getMe(a);
    await getProviders(a);
    expect(seen.map((r) => `${r.method} ${new URL(r.url).pathname}`)).toEqual([
      'GET /api/me',
      'GET /api/auth/providers',
    ]);
  });

  it('putDisplayName, putPreferences and postUnlink', async () => {
    const { api: a, seen } = api(() => json(200, {}));
    await putDisplayName(a, 'Ana');
    await putPreferences(a, { locale: 'es' });
    await postUnlink(a, 'google');
    expect(seen.map((r) => `${r.method} ${new URL(r.url).pathname}`)).toEqual([
      'PUT /api/me/display-name',
      'PUT /api/me/preferences',
      'POST /api/auth/unlink/google',
    ]);
    expect(await seen[0]?.text()).toBe('{"display_name":"Ana"}');
    expect(await seen[1]?.text()).toBe('{"locale":"es"}');
  });
});
