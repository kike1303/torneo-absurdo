import type { Me } from '@torneo/protocol';
import { describe, expect, it, vi } from 'vitest';
import { NEXT_CASES } from '../support/next-cases.js';
import { fakeEvent, redirectOf, writtenCookies } from '../support/event.js';

vi.mock('#lib/server/api/context.js', () => ({
  internalApi: (fetch: typeof globalThis.fetch) => ({ fetch, base: 'http://torneo-server:8080' }),
}));
const { actions, load } = await import('../../src/routes/preferences/+page.server.js');

// 008 RF-D4, plan-d decision 5.
const ORIGIN = 'http://localhost:4173';
const ME = { display_name: 'Ana', locale: 'es', theme: 'light' } as Me;

function server(status: number, body: unknown = ME) {
  const seen: { method: string; url: string; body: string }[] = [];
  const fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = new Request(input, init);
    seen.push({ method: request.method, url: request.url, body: await request.text() });
    return new Response(JSON.stringify(body), {
      status,
      headers: { 'content-type': 'application/json' },
    });
  }) as typeof globalThis.fetch;
  return { fetch, seen };
}

const signedIn = { user: ME, session: 'valid' as const };

function event(form: Record<string, string>, fetch: typeof globalThis.fetch, locals = {}) {
  return fakeEvent({
    url: `${ORIGIN}/es/preferences?/x`,
    form,
    fetch,
    locals: { ...signedIn, ...locals },
  });
}

type Action = (event: never) => unknown;
const theme = actions.theme as Action;
const locale = actions.locale as Action;

describe('actions.theme', () => {
  it('200: saves the profile and writes the theme cookie', async () => {
    const { fetch, seen } = server(200);
    const e = event({ theme: 'light' }, fetch);
    expect(await theme(e as never)).toEqual({ saved: true });
    expect(seen).toEqual([
      {
        method: 'PUT',
        url: 'http://torneo-server:8080/api/me/preferences',
        body: '{"theme":"light"}',
      },
    ]);
    expect(writtenCookies(e)).toEqual([
      {
        name: 'theme',
        value: 'light',
        options: { path: '/', maxAge: 31_536_000, sameSite: 'lax', httpOnly: false },
      },
    ]);
  });

  it('422: fails with the code, the cookie untouched', async () => {
    const { fetch } = server(422, { error: 'http.invalid_body' });
    const e = event({ theme: 'light' }, fetch);
    const result = (await theme(e as never)) as { status: number; data: unknown };
    expect(result.status).toBe(422);
    expect(result.data).toEqual({ code: 'http.invalid_body' });
    expect(writtenCookies(e)).toEqual([]);
  });

  it('401: fails with auth.unauthenticated', async () => {
    const { fetch } = server(401, { error: 'auth.unauthenticated' });
    const result = (await theme(event({ theme: 'dark' }, fetch) as never)) as {
      status: number;
      data: unknown;
    };
    expect(result.status).toBe(401);
    expect(result.data).toEqual({ code: 'auth.unauthenticated' });
  });

  it('a theme outside the list never reaches the API', async () => {
    const { fetch, seen } = server(200);
    const result = (await theme(event({ theme: 'neon' }, fetch) as never)) as { status: number };
    expect(result.status).toBe(422);
    expect(seen).toEqual([]);
  });

  it('without a session never reaches the API', async () => {
    const { fetch, seen } = server(200);
    const result = (await theme(
      event({ theme: 'dark' }, fetch, { user: null, session: 'none' }) as never,
    )) as { status: number; data: unknown };
    expect(result.status).toBe(401);
    expect(result.data).toEqual({ code: 'auth.unauthenticated' });
    expect(seen).toEqual([]);
  });
});

describe('actions.locale', () => {
  it('200: saves the profile, writes the cookie and answers 303 to `to`', async () => {
    const { fetch, seen } = server(200);
    const e = event({ locale: 'en', to: '/en/account' }, fetch);
    expect(await redirectOf(() => locale(e as never))).toEqual({
      status: 303,
      location: '/en/account',
    });
    expect(seen[0]?.body).toBe('{"locale":"en"}');
    expect(writtenCookies(e).map((c) => [c.name, c.value])).toEqual([['locale', 'en']]);
  });

  it('an API error still goes to `to`, without the cookie', async () => {
    const { fetch } = server(503, { error: 'auth.try_again' });
    const e = event({ locale: 'en', to: '/en/' }, fetch);
    expect((await redirectOf(() => locale(e as never))).location).toBe('/en/');
    expect(writtenCookies(e)).toEqual([]);
  });

  it.each(NEXT_CASES)('`to` %j → %j', async (to, expected) => {
    const { fetch } = server(200);
    const form: Record<string, string> = { locale: 'en' };
    if (typeof to === 'string') form.to = to;
    const { location } = await redirectOf(() => locale(event(form, fetch) as never));
    expect(location).toBe(expected ?? '/en/');
  });

  it('a locale outside the list never reaches the API', async () => {
    const { fetch, seen } = server(200);
    const result = (await locale(event({ locale: 'fr', to: '/en/' }, fetch) as never)) as {
      status: number;
    };
    expect(result.status).toBe(422);
    expect(seen).toEqual([]);
  });
});

describe('load', () => {
  it('a GET goes to the account', async () => {
    const e = fakeEvent({ url: `${ORIGIN}/es/preferences` });
    expect(await redirectOf(() => (load as Action)(e as never))).toEqual({
      status: 303,
      location: '/es/account',
    });
  });
});
