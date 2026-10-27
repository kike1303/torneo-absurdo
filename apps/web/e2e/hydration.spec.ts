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
