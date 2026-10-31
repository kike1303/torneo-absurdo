import type { Me, ReauthStatus } from '@torneo/protocol';
import { describe, expect, it, vi } from 'vitest';
import { fakeEvent, redirectOf } from '../support/event.js';
import { actions, load } from '../../src/routes/account/+page.server.js';

// `internalApi` reads `$app/env/private`, whose module checks `src/env.ts` when it is imported and
// does not see `process.env` here. Only the env is replaced: `fakeEvent({ fetch })` answers the API.
vi.mock('$app/env/private', () => ({ INTERNAL_API_URL: 'http://internal.test' }));

// 009 decision 13: the load of /account. Its query never reaches the page or the locale switcher
// (which copies `search`), and it only canonicalizes a GET: without JS, Kit runs the action and
// then this load with the same URL (`?/unlink`), so a redirect there would lose the `fail()`.
const ORIGIN = 'http://localhost:4173';
const CANARY = 'canary-reauth-7f3a';
const me: Me = {
  display_name: 'Ana',
  role: 'user',
  balance: 100,
  providers: ['google', 'dev'],
  locale: null,
  theme: null,
  display_name_is_generated: false,
};
const READY: ReauthStatus = {
  link_ready: true,
  unlink: [
    { provider: 'google', ready: true, reauth_with: ['google'] },
    { provider: 'dev', ready: true, reauth_with: ['google', 'dev'] },
  ],
};
const NOT_READY: ReauthStatus = {
  link_ready: false,
  unlink: READY.unlink.map((u) => ({ ...u, ready: false })),
};

const json = (body: unknown, status = 200) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });

interface Answers {
  reauth?: () => Response;
  unlink?: () => Response;
  rename?: () => Response;
}

/** The internal API: answers by path, and fails on anything a test did not expect. */
function api(answers: Answers = {}): typeof globalThis.fetch {
  return (async (input: string | URL | Request) => {
    const path = new URL(String(input), ORIGIN).pathname;
    if (path === '/api/auth/providers') return json(['google', 'discord', 'dev']);
    if (path === '/api/auth/reauth-status') return (answers.reauth ?? (() => json(READY)))();
    if (path.startsWith('/api/auth/unlink/') && answers.unlink) return answers.unlink();
    if (path === '/api/me/display-name' && answers.rename) return answers.rename();
    throw new Error(`unexpected request: ${path}`);
  }) as typeof globalThis.fetch;
}

type Handler = (event: never) => unknown;
const callLoad = load as unknown as Handler;

function event(
  path: string,
  init: {
    method?: string;
    form?: Record<string, string>;
    answers?: Answers;
    user?: Me | null;
  } = {},
) {
  const user = init.user === undefined ? me : init.user;
  return fakeEvent({
    url: `${ORIGIN}${path}`,
    method: init.method,
    form: init.form,
    fetch: api(init.answers),
    locals: { user, session: user ? 'valid' : 'none' },
  }) as never;
}

const data = (path: string, init?: Parameters<typeof event>[1]) =>
  callLoad(event(path, init)) as Promise<Record<string, unknown>>;

describe('/account load: the query (GET)', () => {
  it('without a session goes to the login', async () => {
    const { status, location } = await redirectOf(() =>
      callLoad(event('/es/account', { user: null })),
    );
    expect(status).toBe(303);
    expect(location).toBe('/es/login?next=%2Fes%2Faccount');
  });

  it.each(['/es/account', '/es/account?reauth=1'])('%s stays as it is', async (path) => {
    expect(await data(path)).toMatchObject({ state: 'ready' });
  });

  it.each([
    [`/es/account?reauth=${CANARY}`, '/es/account?reauth=1'],
    [`/es/account?reauth=1&x=${CANARY}`, '/es/account?reauth=1'],
    [`/es/account?x=1&reauth=${CANARY}`, '/es/account?reauth=1'],
    ['/es/account?reauth', '/es/account?reauth=1'],
    ['/es/account?reauth=', '/es/account?reauth=1'],
    [`/es/account?x=${CANARY}`, '/es/account'],
    ['/en/account?x=1', '/en/account'],
    ['/es/account?/unlink', '/es/account'],
  ])('%s → %s, with nothing of the URL', async (path, expected) => {
    const { status, location } = await redirectOf(() => callLoad(event(path)));
    expect(status).toBe(303);
    expect(location).toBe(expected);
    expect(location).not.toContain(CANARY);
  });

  it('canonicalizes before it looks at the session: no value survives an outage', async () => {
    const e = fakeEvent({
      url: `${ORIGIN}/es/account?reauth=${CANARY}`,
      locals: { user: null, session: 'unavailable' },
    }) as never;
    expect((await redirectOf(() => callLoad(e))).location).toBe('/es/account?reauth=1');
  });
});

describe('/account load: after an action without JS (POST)', () => {
  it.each([
    ['?/unlink', { provider: 'google' }],
    ['?/rename', { display_name: 'Otro' }],
  ])('%s does not redirect and returns its data', async (search, form) => {
    const result = await data(`/es/account${search}`, { form });
    expect(result).toMatchObject({ state: 'ready', displayName: 'Ana', reauth: READY });
  });

  it('reauthDone is false: the URL of the action has no reauth', async () => {
    expect(await data('/es/account?/unlink', { form: { provider: 'google' } })).toMatchObject({
      reauthDone: false,
    });
  });
});

describe('/account actions keep their failure', () => {
  const callAction = (name: 'unlink' | 'rename', e: never) =>
    (actions[name] as unknown as Handler)(e) as Promise<{
      status: number;
      data: Record<string, unknown>;
    }>;

  it('a 403 of unlink is fail(403, { code, provider })', async () => {
    const e = event('/es/account?/unlink', {
      form: { provider: 'google' },
      answers: { unlink: () => json({ error: 'auth.reauth_required' }, 403) },
    });
    const result = await callAction('unlink', e);
    expect(result.status).toBe(403);
    expect(result.data).toEqual({ code: 'auth.reauth_required', provider: 'google' });
  });

  it('a 409 of rename is fail(409, { code, value })', async () => {
    const e = event('/es/account?/rename', {
      form: { display_name: 'Otro' },
      answers: { rename: () => json({ error: 'profile.display_name_taken' }, 409) },
    });
    const result = await callAction('rename', e);
    expect(result.status).toBe(409);
    expect(result.data).toEqual({ code: 'profile.display_name_taken', value: 'Otro' });
  });
});

describe('/account load: reauth and reauthDone', () => {
  it('passes reauth-status through', async () => {
    const answers = { reauth: () => json(NOT_READY) };
    expect(await data('/es/account', { answers })).toMatchObject({ reauth: NOT_READY });
  });

  it('reauthDone needs the parameter and link_ready', async () => {
    const notReady = { reauth: () => json(NOT_READY) };
    expect(await data('/es/account?reauth=1')).toMatchObject({ reauthDone: true });
    expect(await data('/es/account')).toMatchObject({ reauthDone: false });
    expect(await data('/es/account?reauth=1', { answers: notReady })).toMatchObject({
      reauthDone: false,
    });
  });

  it.each([
    ['a 503', () => json({ error: 'auth.try_again' }, 503)],
    ['a 401', () => json({ error: 'auth.unauthenticated' }, 401)],
    ['an invalid shape', () => json({ link_ready: 'yes', unlink: [] })],
    ['a body that is not JSON', () => new Response('nope', { status: 200 })],
  ])('with %s, reauth is null and the page still works', async (_name, reauth) => {
    const result = await data('/es/account?reauth=1', { answers: { reauth } });
    expect(result).toMatchObject({ state: 'ready', reauth: null, reauthDone: false });
  });

  it('a network error gives null too', async () => {
    const reauth = () => {
      throw new Error('down');
    };
    expect(await data('/es/account', { answers: { reauth } })).toMatchObject({ reauth: null });
  });
});
