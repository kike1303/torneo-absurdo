import { expect, test, type Page } from '@playwright/test';
import { waitForHydration } from './support/hydration.js';

async function collectViolations(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const store: string[] = [];
    (window as unknown as { __csp: string[] }).__csp = store;
    document.addEventListener('securitypolicyviolation', (event) => {
      store.push(`${event.violatedDirective} ${event.blockedURI}`);
    });
  });
}

const violations = (page: Page) =>
  page.evaluate(() => (window as unknown as { __csp: string[] }).__csp);

test('RF-A10: no CSP violations on /es/ and /en/, and the theme script runs', async ({ page }) => {
  await collectViolations(page);
  await page
    .context()
    .addCookies([{ name: 'theme', value: 'system', url: 'http://localhost:4173' }]);
  for (const path of ['/es/', '/en/']) {
    await page.goto(path);
    await expect(page.locator('html')).not.toHaveAttribute('data-theme', 'system');
    expect(await violations(page), path).toEqual([]);
  }
});

test('RF-A10: no CSP violations in the catalog with a menu and a dialog open', async ({ page }) => {
  await collectViolations(page);
  await page.goto('/es/dev/components');
  await waitForHydration(page);
  await page.getByTestId('dropdown-trigger').click();
  await expect(page.getByTestId('dropdown-content')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByTestId('dialog-trigger').click();
  await expect(page.getByTestId('dialog-content')).toBeVisible();
  await page.waitForTimeout(300);
  expect(await violations(page)).toEqual([]);
});

// 002 RF-A18 and ADR-0012 B6.1: Pixi runs under the CSP as it is, in Chromium and WebKit. If the
// engine has no WebGL (WebKit on Linux), the route must still load the deferred chunks and run
// `pixi.js/unsafe-eval` (data-renderer-loaded) before it reports webgl_unavailable.
test('RF-A18: no CSP violations on /es/dev/arena while loading and 5 s of playback', async ({
  page,
}) => {
  await collectViolations(page);
  await page.goto('/es/dev/arena');
  const stage = page.getByTestId('fight-stage');
  await expect(stage).toHaveAttribute('data-renderer-loaded', '', { timeout: 15_000 });
  await expect(stage).toHaveAttribute('data-state', /^(playing|error)$/, { timeout: 15_000 });
  if ((await stage.getAttribute('data-state')) === 'error') {
    await expect(stage).toHaveAttribute('data-error', 'webgl_unavailable');
    test.info().annotations.push({ type: 'limited', description: 'no WebGL in this engine' });
  } else {
    await page.waitForTimeout(5000);
    await expect(stage).toHaveAttribute('data-state', 'playing');
  }
  expect(await violations(page)).toEqual([]);
});
