import { expect, test } from '@playwright/test';
import { waitForHydration } from './support/hydration.js';

// `html[data-hydrated]` is the signal waitForHydration relies on (TA-12): it must never come from
// the SSR, only from the client mount.

test('the SSR HTML does not carry data-hydrated', async ({ request }) => {
  for (const path of ['/es/', '/en/dev/components', '/es/no-existe']) {
    const html = await (await request.get(path)).text();
    expect(html, path).toContain('<html');
    expect(html, path).not.toContain('data-hydrated');
  }
});

test('data-hydrated appears only once the client has mounted', async ({ page }) => {
  let release!: () => void;
  const gate = new Promise<void>((resolve) => (release = resolve));
  await page.route('**/_app/immutable/**', async (route) => {
    await gate;
    await route.continue();
  });

  // `commit`: with the chunks held, `domcontentloaded` never fires.
  await page.goto('/es/', { waitUntil: 'commit' });
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'dark');
  expect(await page.locator('html').getAttribute('data-hydrated')).toBeNull();

  release();
  await waitForHydration(page);
  await expect(page.locator('html')).toHaveAttribute('data-hydrated', '');
});

// 011 RF17: the same text on both sides at first, in UTC and with its zone, and local from `onMount`.
test.describe('the date of a past fight', { tag: '@replays' }, () => {
  test.use({ timezoneId: 'America/Mexico_City', locale: 'es-MX' });

  test('UTC in the SSR, local once mounted, without an error', async ({ page, request }) => {
    const html = await (await request.get('/es/fights')).text();
    expect(html).toContain('UTC');

    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    page.on('console', (message) => {
      if (message.type() === 'error') errors.push(message.text());
    });
    await page.goto('/es/fights');
    await waitForHydration(page);
    const date = page.getByTestId('fight-date').first();
    await expect(date).toHaveAttribute('data-local', '');
    await expect(date).not.toContainText('UTC');
    expect(errors).toEqual([]);
  });
});
