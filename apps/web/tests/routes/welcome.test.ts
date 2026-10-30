import type { Me } from '@torneo/protocol';
import { describe, expect, it } from 'vitest';
import { NEXT_CASES } from '../support/next-cases.js';
import { fakeEvent, redirectOf, writtenCookies } from '../support/event.js';
import { load } from '../../src/routes/welcome/+page.server.js';

// 008 RF-D4 ("on login, a profile value rewrites the cookie"), plan-d decisions 5 and 9.
const ORIGIN = 'http://localhost:4173';
const me = (over: Partial<Me> = {}): Me => ({
  display_name: 'Ana',
  role: 'user',
  balance: 100,
  providers: ['dev'],
  locale: null,
  theme: null,
  display_name_is_generated: false,
  ...over,
});

type Load = (event: never) => unknown;

function run(path: string, user: Me | null) {
  const e = fakeEvent({
    url: `${ORIGIN}${path}`,
    locals: { user, session: user ? 'valid' : 'none' },
  });
  return redirectOf(() => (load as Load)(e as never)).then((r) => ({ ...r, e }));
}

describe('/welcome load', () => {
  it('without a session goes to the login', async () => {
    const { status, location } = await run('/es/welcome?next=/es/account', null);
    expect(status).toBe(303);
    expect(location).toBe('/es/login');
  });

  it('a generated alias goes to the onboarding with the validated next', async () => {
    const { location } = await run(
      '/es/welcome?next=%2Fes%2Faccount',
      me({ display_name_is_generated: true }),
    );
    expect(location).toBe('/es/onboarding?next=%2Fes%2Faccount');
  });

  it('a chosen alias goes straight to next', async () => {
    expect((await run('/es/welcome?next=/es/account', me())).location).toBe('/es/account');
  });

  it('rewrites the cookies from the profile; null leaves them alone', async () => {
    const { e } = await run('/es/welcome', me({ locale: 'en', theme: 'light' }));
    expect(writtenCookies(e).map((c) => [c.name, c.value])).toEqual([
      ['locale', 'en'],
      ['theme', 'light'],
    ]);
    const { e: none } = await run('/es/welcome', me());
    expect(writtenCookies(none)).toEqual([]);
  });

  it('change 7 of architect: the destination keeps the language of its own prefix', async () => {
    const { location, e } = await run('/en/welcome?next=/en/account', me({ locale: 'es' }));
    expect(location).toBe('/en/account');
    expect(writtenCookies(e).map((c) => [c.name, c.value])).toEqual([['locale', 'es']]);
  });

  it.each(NEXT_CASES)('next %j → %j', async (next, expected) => {
    const query = typeof next === 'string' ? `?next=${encodeURIComponent(next)}` : '';
    expect((await run(`/es/welcome${query}`, me())).location).toBe(expected ?? '/es/');
    const generated = await run(`/es/welcome${query}`, me({ display_name_is_generated: true }));
    expect(generated.location).toBe(
      `/es/onboarding?next=${encodeURIComponent(expected ?? '/es/')}`,
    );
  });
});
