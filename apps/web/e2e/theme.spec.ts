import { expect, test, type Page } from '@playwright/test';

const ORIGIN = 'http://localhost:4173';
const YEAR = 31_536_000;

async function themeCookie(page: Page, value: string): Promise<void> {
  await page.context().addCookies([{ name: 'theme', value, url: ORIGIN }]);
}

/** Records `data-theme` at the first animation frame, before the first paint (risk "test 5"). */
async function recordFirstFrame(page: Page): Promise<void> {
  await page.addInitScript(() => {
    requestAnimationFrame(() => {
      (window as unknown as { __firstFrameTheme?: string }).__firstFrameTheme =
        document.documentElement.dataset.theme;
    });
  });
}

test.describe('test 5: no flicker', () => {
  for (const value of ['dark', 'light'] as const) {
    test(`SSR already carries theme=${value}`, async ({ request }) => {
      const html = await (
        await request.get('/es/', { headers: { cookie: `theme=${value}` } })
      ).text();
      expect(html).toContain(`data-theme="${value}"`);
      expect(html).toContain(`<meta name="color-scheme" content="${value}" />`);
      expect(html.match(/name="theme-color"/g)).toHaveLength(1);
    });
  }

  test('without cookie the SSR is dark', async ({ request }) => {
    const html = await (await request.get('/es/')).text();
    expect(html).toContain('data-theme="dark"');
  });

  for (const scheme of ['light', 'dark'] as const) {
    test(`system follows prefers-color-scheme=${scheme} before the first paint`, async ({
      page,
      request,
    }) => {
      const html = await (
        await request.get('/es/', { headers: { cookie: 'theme=system' } })
      ).text();
      expect(html).toContain('<meta name="color-scheme" content="light dark" />');
      expect(html.match(/name="theme-color" media=/g)).toHaveLength(2);

      await page.emulateMedia({ colorScheme: scheme });
      await themeCookie(page, 'system');
      await recordFirstFrame(page);
      await page.goto('/es/');
      const first = await page.evaluate(
        () => (window as unknown as { __firstFrameTheme?: string }).__firstFrameTheme,
      );
      expect(first).toBe(scheme);

      const other = scheme === 'light' ? 'dark' : 'light';
      await page.emulateMedia({ colorScheme: other });
      await expect(page.locator('html')).toHaveAttribute('data-theme', other);
    });
  }
});

test('RF-A8: the switcher changes the theme live and writes the cookie', async ({
  page,
  context,
}) => {
  await page.goto('/es/');
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  const navigations: string[] = [];
  page.on('framenavigated', (frame) => {
    if (frame === page.mainFrame()) navigations.push(frame.url());
  });

  await page.getByTestId('theme-switcher').click();
  const light = page.getByTestId('theme-option-light');
  await expect(page.getByTestId('theme-option-dark')).toHaveAttribute('aria-checked', 'true');
  await light.click();

  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect(page.locator('meta[name="color-scheme"]')).toHaveAttribute('content', 'light');
  await expect(page.locator('meta[name="theme-color"]')).toHaveAttribute('content', '#ffffff');
  expect(navigations).toEqual([]);

  const cookie = (await context.cookies()).find((c) => c.name === 'theme');
  expect(cookie?.value).toBe('light');
  expect(cookie?.path).toBe('/');
  expect(cookie?.sameSite).toBe('Lax');
  expect(cookie?.httpOnly).toBe(false);
  const remaining = (cookie?.expires ?? 0) - Date.now() / 1000;
  expect(remaining).toBeGreaterThan(YEAR - 120);

  await page.getByTestId('theme-switcher').click();
  await expect(page.getByTestId('theme-option-light')).toHaveAttribute('aria-checked', 'true');
});

test.describe('test 6 / RF-A9: catalog override', () => {
  test('?theme=light on /dev/** wins over the cookie and writes nothing', async ({
    page,
    context,
  }) => {
    await themeCookie(page, 'dark');
    await page.goto('/es/dev/components?theme=light');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
    const cookie = (await context.cookies()).find((c) => c.name === 'theme');
    expect(cookie?.value).toBe('dark');
  });

  test('?theme=light outside /dev/** has no effect', async ({ page }) => {
    await page.goto('/es/?theme=light');
    await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  });
});
