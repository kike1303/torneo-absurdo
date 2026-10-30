import { expect, test } from '@playwright/test';
import { waitForHydration } from './support/hydration.js';
import { header, raw } from './support/raw-http.js';

const PORT = 4173;
const YEAR = 31_536_000;

test.describe('locale redirect (RF-A3)', () => {
  test('/ and paths without prefix: cookie → Accept-Language → en, relative 307', async () => {
    const plain = await raw(PORT, '/');
    expect(plain.status).toBe(307);
    expect(header(plain, 'location')).toBe('/en/');
    expect(header(plain, 'vary')).toBe('Cookie, Accept-Language');
    expect(header(plain, 'cache-control')).toBe('private, no-store');

    const accept = await raw(PORT, '/dev/components?theme=light', {
      headers: { 'accept-language': 'es-MX,es;q=0.9,en;q=0.5' },
    });
    expect(header(accept, 'location')).toBe('/es/dev/components?theme=light');

    const cookie = await raw(PORT, '/', {
      headers: { cookie: 'locale=en', 'accept-language': 'es' },
    });
    expect(header(cookie, 'location')).toBe('/en/');

    const invalid = await raw(PORT, '/', {
      headers: { cookie: 'locale=xx', 'accept-language': 'es' },
    });
    expect(header(invalid, 'location')).toBe('/es/');
  });

  test('never redirects to another host', async () => {
    for (const path of ['//evil.example/', '/\\evil.example', '///evil.example/x?y=1']) {
      const res = await raw(PORT, path);
      expect(res.status, path).toBe(307);
      const location = header(res, 'location') ?? '';
      expect(location, path).toMatch(/^\/(es|en)\/(?![/\\])/);
    }
  });

  test('excluded paths are not redirected; /apix is', async () => {
    expect((await raw(PORT, '/healthz')).status).toBe(200);
    for (const path of ['/api/x', '/ws']) {
      const res = await raw(PORT, path);
      expect(res.status, path).not.toBe(307);
      expect(header(res, 'location'), path).toBeUndefined();
    }
    expect((await raw(PORT, '/apix')).status).toBe(307);
  });

  // With the strategy ['url', 'baseLocale'] Paraglide's middleware never redirects: a URL without a
  // prefix is answered by our localeRedirect before it, and a URL with a prefix decides the locale.
  // This test pins, case by case, which handle answers and that no Location is absolute (QA H4).
  test('document requests with a forged Host: only localeRedirect redirects, always relative', async () => {
    const OWN_REDIRECT = 'localeRedirect';
    const cases: [string, number, string | undefined][] = [
      ['/', 307, '/en/'],
      ['/dev/components', 307, '/en/dev/components'],
      ['//evil.example/', 307, '/en/evil.example/'],
      ['/\\evil.example', 307, '/en/evil.example'],
      ['/EN/', 307, '/en/EN/'],
      ['/es', 200, undefined],
      ['/en', 200, undefined],
      ['/es/', 200, undefined],
      ['/es//evil.example/', 404, undefined],
    ];
    for (const [path, status, location] of cases) {
      const res = await raw(PORT, path, {
        headers: { host: 'evil.example', 'sec-fetch-dest': 'document' },
      });
      expect(res.status, path).toBe(status);
      expect(header(res, 'location'), path).toBe(location);
      if (status === 307) {
        const handler =
          header(res, 'cache-control') === 'private, no-store' &&
          header(res, 'vary') === 'Cookie, Accept-Language'
            ? OWN_REDIRECT
            : 'other';
        expect(handler, path).toBe(OWN_REDIRECT);
        expect(location, path).toMatch(/^\/(?![/\\])/);
        expect(location, path).not.toMatch(/^[a-z][a-z0-9+.-]*:/i);
      }
    }
  });

  test('<html lang> follows the route', async ({ page }) => {
    await page.goto('/es/');
    await expect(page.locator('html')).toHaveAttribute('lang', 'es');
    await page.goto('/en/');
    await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  });
});

test.describe('locale cookie and switcher (RF-A4, RF-A5)', () => {
  test('choosing a language writes the cookie and keeps path and query', async ({
    page,
    context,
  }) => {
    await page.goto('/en/dev/components?theme=light');
    await waitForHydration(page);
    await page.getByTestId('header-switchers').getByTestId('locale-switcher').click();
    await page.getByTestId('locale-option-es').click();
    await page.waitForURL('**/es/dev/components?theme=light');
    await expect(page.locator('html')).toHaveAttribute('lang', 'es');

    const cookie = (await context.cookies()).find((c) => c.name === 'locale');
    expect(cookie).toBeDefined();
    expect(cookie?.value).toBe('es');
    expect(cookie?.path).toBe('/');
    expect(cookie?.sameSite).toBe('Lax');
    expect(cookie?.httpOnly).toBe(false);
    expect(cookie?.secure).toBe(false);
    const remaining = (cookie?.expires ?? 0) - Date.now() / 1000;
    expect(remaining).toBeGreaterThan(YEAR - 120);
    expect(remaining).toBeLessThanOrEqual(YEAR + 5);

    // A5: after choosing es, `/` goes to /es/ even if Accept-Language asks for en.
    const res = await page.request.get('/', {
      maxRedirects: 0,
      headers: { 'accept-language': 'en-US,en;q=0.9' },
    });
    expect(res.status()).toBe(307);
    expect(res.headers()['location']).toBe('/es/');
  });

  test('visiting /en/ by a link does not change the cookie', async ({ page, context }) => {
    await context.addCookies([{ name: 'locale', value: 'es', url: 'http://localhost:4173' }]);
    await page.goto('/es/');
    await page.goto('/en/');
    await waitForHydration(page);
    await page
      .getByRole('link', { name: /Absurd Tournament/ })
      .first()
      .click();
    const cookie = (await context.cookies()).find((c) => c.name === 'locale');
    expect(cookie?.value).toBe('es');
  });
});

test.describe('pages on excluded paths (QA H2)', () => {
  // The web directly: through the edge, /api and /ws reach the server (008 plan-d decision 10).
  const WEB = 4175;
  const PATHS = ['/api', '/api/x', '/ws', '/healthz/x', '/healthz//'];

  for (const method of ['GET', 'POST'] as const) {
    test(`${method}: 404s have a valid lang and no Paraglide marker`, async () => {
      for (const [cookie, lang] of [
        [undefined, 'en'],
        ['locale=es', 'es'],
        ['locale=xx', 'en'],
      ] as const) {
        for (const path of PATHS) {
          const res = await raw(WEB, path, {
            method,
            headers: {
              ...(cookie ? { cookie } : {}),
              ...(method === 'POST'
                ? { 'content-type': 'application/json', origin: `http://localhost:${PORT}` }
                : {}),
            },
            ...(method === 'POST' ? { body: '{}' } : {}),
          });
          const label = `${method} ${path} ${cookie ?? 'no cookie'}`;
          expect(res.status, label).toBeGreaterThanOrEqual(400);
          expect(header(res, 'location'), label).toBeUndefined();
          expect(header(res, 'content-type'), label).toContain('text/html');
          expect(res.body, label).not.toContain('%paraglide.');
          expect(res.body, label).toContain(`<html lang="${lang}"`);
        }
      }
    });
  }
});
