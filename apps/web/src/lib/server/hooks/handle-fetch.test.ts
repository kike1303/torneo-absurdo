import { describe, expect, it } from 'vitest';
import { createHandleFetch, isInternalUrl, sessionCookies } from './handle-fetch.js';

// 008 RF-D1, RF-D2, T14 (web) and T16 (plan-d decision 2).
const INTERNAL = 'http://torneo-server:8080';
const SECRET = 'kq3Z0m9sV7cR2xW1pL8nB4tY6uE5aJ0dF3hG7iK9oQs';
const PROD = 'https://torneo.example';
const LOOPBACK = 'http://localhost:4173';
const TOKEN = 'c2Vzc2lvbi10b2tlbi1vZi10aGUtdXNlcg';

interface Call {
  request: Request;
}

function setup(publicOrigin = PROD) {
  const calls: Call[] = [];
  const handle = createHandleFetch({ internalApiUrl: INTERNAL, secret: SECRET, publicOrigin });
  const fetch = (async (input: RequestInfo | URL) => {
    calls.push({ request: input as Request });
    return new Response(null, { status: 204 });
  }) as typeof globalThis.fetch;
  return {
    calls,
    run: async (
      url: string,
      init: RequestInit = {},
      browser: Record<string, string> = {},
      clientAddress = '203.0.113.5',
    ) => {
      const event = {
        request: new Request(`${publicOrigin}/es/account`, { headers: browser }),
        getClientAddress: () => clientAddress,
      };
      await handle({
        event: event as never,
        request: new Request(url, init),
        fetch,
      });
      const last = calls.at(-1);
      if (!last) throw new Error('fetch was not called');
      return last.request;
    },
  };
}

const INTERNAL_HEADERS = [
  'cookie',
  'x-internal-secret',
  'x-client-ip',
  'x-client-origin',
  'x-client-fetch-site',
];
const browserHeaders = {
  cookie: `__Host-session=${TOKEN}; theme=dark; other=canary-other-cookie`,
  origin: PROD,
  'sec-fetch-site': 'same-origin',
  'x-forwarded-for': '198.51.100.99',
};

describe('isInternalUrl: the prefix with its `/`', () => {
  it.each([
    [`${INTERNAL}/api/me`, true],
    [`${INTERNAL}/`, true],
    // Look-alikes of the internal origin: userinfo, a longer port, a longer host. `Request`
    // refuses to build some of them, so the predicate is checked on the text too.
    ['http://torneo-server:8080@evil.example/', false],
    ['http://torneo-server:80800/', false],
    ['http://torneo-server:8080.evil.example/', false],
    ['http://torneo-server:8080', false],
    ['https://example.com/', false],
  ])('%s → %s', (url, expected) => expect(isInternalUrl(url, INTERNAL)).toBe(expected));
});

describe('T16: a fetch that is not to INTERNAL_API_URL carries nothing internal', () => {
  it.each([
    'https://example.com/',
    'http://torneo-server.evil.example:8080/api/me',
    'http://evil.example/http://torneo-server:8080/api/me',
    'https://torneo-server:8080/api/me',
    'http://torneo-server:8081/api/me',
    'http://torneo-server/api/me',
  ])('%s', async (url) => {
    const { run } = setup();
    const sent = await run(
      url,
      {
        headers: {
          cookie: 'planted=1',
          'x-internal-secret': 'planted-secret',
          'x-client-ip': '192.0.2.1',
          'x-client-origin': PROD,
          'x-client-fetch-site': 'same-origin',
          'x-client-anything': 'x',
          accept: 'application/json',
        },
      },
      browserHeaders,
    );
    for (const name of [...INTERNAL_HEADERS, 'x-client-anything']) {
      expect(sent.headers.has(name), name).toBe(false);
    }
    expect(sent.headers.get('accept')).toBe('application/json');
  });
});

describe('an internal fetch', () => {
  it('carries the session cookie, the secret, the client IP and the browser Origin', async () => {
    const { run } = setup();
    const sent = await run(`${INTERNAL}/api/me`, {}, browserHeaders);
    expect(sent.headers.get('cookie')).toBe(`__Host-session=${TOKEN}`);
    expect(sent.headers.get('x-internal-secret')).toBe(SECRET);
    expect(sent.headers.get('x-client-ip')).toBe('203.0.113.5');
    expect(sent.headers.get('x-client-origin')).toBe(PROD);
    expect(sent.headers.get('x-client-fetch-site')).toBe('same-origin');
    expect(sent.url).toBe(`${INTERNAL}/api/me`);
  });

  it('never forwards the other browser cookies', async () => {
    const { run } = setup();
    const sent = await run(`${INTERNAL}/api/me`, {}, browserHeaders);
    expect(sent.headers.get('cookie')).not.toContain('theme');
    expect(sent.headers.get('cookie')).not.toContain('canary-other-cookie');
  });

  it('omits Origin and Sec-Fetch-Site when the browser did not send them', async () => {
    const { run } = setup();
    const sent = await run(`${INTERNAL}/api/me`, {}, { cookie: `__Host-session=${TOKEN}` });
    expect(sent.headers.has('x-client-origin')).toBe(false);
    expect(sent.headers.has('x-client-fetch-site')).toBe(false);
  });

  it('replaces whatever internal headers the caller planted', async () => {
    const { run } = setup();
    const sent = await run(
      `${INTERNAL}/api/me`,
      {
        headers: {
          cookie: 'planted=1',
          'x-internal-secret': 'planted',
          'x-client-ip': '192.0.2.1',
          'x-client-origin': 'https://evil.example',
          'x-client-extra': 'x',
        },
      },
      { cookie: `__Host-session=${TOKEN}` },
    );
    expect(sent.headers.get('cookie')).toBe(`__Host-session=${TOKEN}`);
    expect(sent.headers.get('x-internal-secret')).toBe(SECRET);
    expect(sent.headers.get('x-client-ip')).toBe('203.0.113.5');
    expect(sent.headers.has('x-client-origin')).toBe(false);
    expect(sent.headers.has('x-client-extra')).toBe(false);
  });

  it('without a session cookie sends no cookie header at all', async () => {
    const { run } = setup();
    const sent = await run(`${INTERNAL}/api/auth/providers`, {}, { cookie: 'theme=dark' });
    expect(sent.headers.has('cookie')).toBe(false);
    expect(sent.headers.get('x-internal-secret')).toBe(SECRET);
  });

  it('T14 (web): the client IP is getClientAddress(), never a forged X-Forwarded-For', async () => {
    const { run } = setup();
    const sent = await run(
      `${INTERNAL}/api/me`,
      {},
      { 'x-forwarded-for': '198.51.100.99, 10.0.0.1', cookie: `__Host-session=${TOKEN}` },
      '2001:db8:1:2::1',
    );
    expect(sent.headers.get('x-client-ip')).toBe('2001:db8:1:2::1');
    expect(sent.headers.has('x-forwarded-for')).toBe(false);
  });

  it("uses redirect: 'manual', so a form action sees the 303", async () => {
    const { run } = setup();
    const sent = await run(`${INTERNAL}/api/auth/unlink/google`, { method: 'POST' });
    expect(sent.redirect).toBe('manual');
    expect(sent.method).toBe('POST');
  });

  it('keeps the method and the body', async () => {
    const { run } = setup();
    const sent = await run(`${INTERNAL}/api/me/display-name`, {
      method: 'PUT',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ display_name: 'x' }),
    });
    expect(sent.method).toBe('PUT');
    expect(sent.headers.get('content-type')).toBe('application/json');
    expect(await sent.text()).toBe('{"display_name":"x"}');
  });
});

// QA of phase D: Kit's own fetch adds every browser cookie (and every `event.cookies.set`) when
// the target's hostname matches `event.url.hostname`, which comes from `Host`, unless the
// request says `credentials: 'omit'`. The end-to-end check is e2e/internal-fetch.spec.ts.
describe("credentials: 'omit', so Kit never adds the browser's cookies", () => {
  it.each([
    `${INTERNAL}/api/me`,
    `${INTERNAL}/api/auth/providers`,
    'https://example.com/',
    'http://torneo-server.evil.example:8080/api/me',
  ])('%s', async (url) => {
    const { run } = setup();
    const sent = await run(url, { credentials: 'include' }, browserHeaders);
    expect(sent.credentials).toBe('omit');
  });
});

describe('RF-D2: the session cookie the web forwards', () => {
  it.each([
    // Production policy: only __Host-session, also on a loopback origin.
    [PROD, `__Host-session=${TOKEN}`, [['__Host-session', TOKEN]]],
    [PROD, `ta_dev_session=${TOKEN}`, []],
    [LOOPBACK, `ta_dev_session=${TOKEN}`, [['ta_dev_session', TOKEN]]],
    [LOOPBACK, `__Host-session=${TOKEN}`, [['__Host-session', TOKEN]]],
    [
      LOOPBACK,
      `__Host-session=${TOKEN}; ta_dev_session=${TOKEN}x`,
      [
        ['__Host-session', TOKEN],
        ['ta_dev_session', `${TOKEN}x`],
      ],
    ],
    // `http` loopback only: https://localhost is "outside loopback" (RF-B18).
    ['https://localhost:4173', `ta_dev_session=${TOKEN}`, []],
    // Values the server never issues are dropped: nothing can inject another `;` or cookie.
    [PROD, '__Host-session=a;b=c', [['__Host-session', 'a']]],
    [PROD, '__Host-session="abc"', []],
    [PROD, '__Host-session=a%3Bb', []],
    [PROD, `__Host-session=${'a'.repeat(129)}`, []],
    [PROD, `__Host-session=${'a'.repeat(128)}`, [['__Host-session', 'a'.repeat(128)]]],
    [PROD, '__Host-session=', []],
    // A repeated cookie counts as absent, like on the server.
    [PROD, `__Host-session=${TOKEN}; __Host-session=${TOKEN}`, []],
    // Exact names only.
    [PROD, `__host-session=${TOKEN}; x__Host-session=${TOKEN}`, []],
    [PROD, null, []],
  ] as const)('%s, %s', (origin, header, expected) => {
    expect(sessionCookies(header, origin)).toEqual(expected);
  });

  it('a cookie with an injected `;` reaches the server as one clean cookie', async () => {
    const { run } = setup();
    const sent = await run(`${INTERNAL}/api/me`, {}, { cookie: '__Host-session=a;admin=1' });
    expect(sent.headers.get('cookie')).toBe('__Host-session=a');
  });
});
