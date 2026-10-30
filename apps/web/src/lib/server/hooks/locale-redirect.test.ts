import { describe, expect, it, vi } from 'vitest';
import { fakeEvent, type FakeEventInit } from '../../../../tests/support/event.js';
import { localeLocation, localeRedirect } from './locale-redirect.js';

const ORIGIN = 'http://localhost:4173';

async function run(path: string, init: Partial<FakeEventInit> = {}) {
  const resolve = vi.fn(async () => new Response('resolved'));
  const event = fakeEvent({ url: `${ORIGIN}${path}`, ...init });
  const response = await localeRedirect({ event, resolve });
  return { response, resolve, event };
}

describe('localeRedirect with a session (008 RF-D4)', () => {
  const user = (locale: 'en' | 'es' | null) => ({ user: { locale } as App.Locals['user'] });
  it('the profile language wins over the cookie and Accept-Language', async () => {
    const { response } = await run('/', {
      headers: { 'accept-language': 'en' },
      cookies: { locale: 'en' },
      locals: user('es'),
    });
    expect(response.headers.get('location')).toBe('/es/');
  });
  it('a profile without a language defers to the cookie', async () => {
    const { response } = await run('/', { cookies: { locale: 'es' }, locals: user(null) });
    expect(response.headers.get('location')).toBe('/es/');
  });
  it('a prefixed path still decides by its URL', async () => {
    const { resolve } = await run('/en/', { locals: user('es') });
    expect(resolve).toHaveBeenCalled();
  });
});

describe('localeRedirect', () => {
  it('redirects / with a relative Location by cookie, Accept-Language, then en', async () => {
    expect((await run('/')).response.headers.get('location')).toBe('/en/');
    expect(
      (await run('/', { headers: { 'accept-language': 'es-MX' } })).response.headers.get(
        'location',
      ),
    ).toBe('/es/');
    expect(
      (
        await run('/', { headers: { 'accept-language': 'es' }, cookies: { locale: 'en' } })
      ).response.headers.get('location'),
    ).toBe('/en/');
    expect((await run('/', { cookies: { locale: 'xx' } })).response.headers.get('location')).toBe(
      '/en/',
    );
  });

  it('answers 307 with Vary and no-store, without calling resolve', async () => {
    const { response, resolve } = await run('/dev/components?theme=light');
    expect(response.status).toBe(307);
    expect(response.headers.get('location')).toBe('/en/dev/components?theme=light');
    expect(response.headers.get('vary')).toBe('Cookie, Accept-Language');
    expect(response.headers.get('cache-control')).toBe('private, no-store');
    expect(resolve).not.toHaveBeenCalled();
  });

  it('never redirects off-site for //host and /\\host', async () => {
    for (const path of ['//evil.example/', '/\\evil.example', '///evil.example']) {
      const { response } = await run(path);
      const location = response.headers.get('location') ?? '';
      expect(response.status).toBe(307);
      expect(location.startsWith('/en/')).toBe(true);
      expect(location.startsWith('/en//')).toBe(false);
    }
  });

  it('keeps CR/LF percent-encoded', async () => {
    const { response } = await run('/a%0d%0aSet-Cookie:%20x=1');
    const location = response.headers.get('location') ?? '';
    expect(location).toBe('/en/a%0d%0aSet-Cookie:%20x=1');
    expect(location).not.toMatch(/[\r\n]/);
  });

  it.each([
    '/es/',
    '/en/',
    '/es',
    '/en',
    '/es/dev/components',
    '/healthz',
    '/api',
    '/api/x',
    '/ws',
  ])('resolves %s', async (path) => {
    const { response, resolve } = await run(path);
    expect(resolve).toHaveBeenCalledOnce();
    expect(response.status).toBe(200);
  });

  it.each(['/apix', '/healthzz', '/wsx', '/EN/', '/esx'])('redirects %s', async (path) => {
    const { response, resolve } = await run(path);
    expect(response.status).toBe(307);
    expect(resolve).not.toHaveBeenCalled();
  });
});

describe('localeLocation', () => {
  it('builds /<locale>/ + path without leading slashes + search', () => {
    expect(localeLocation('en', '/', '')).toBe('/en/');
    expect(localeLocation('es', '//evil.example/', '?q=1')).toBe('/es/evil.example/?q=1');
    expect(localeLocation('es', '/\\evil.example', '')).toBe('/es/evil.example');
  });
});
