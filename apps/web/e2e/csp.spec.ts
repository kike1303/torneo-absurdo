import { expect, test, type Page } from '@playwright/test';
import { login as authLogin, test as authTest } from './auth/fixtures.js';
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
// TA-34: on CI, `browser.newContext` sometimes never answers here, always right after the catalog
// test above, so this test dies before its first line. The cause is unproven. One retry runs in a
// new worker with a new browser, so it only protects the test that follows the catalog test: move
// it if the order changes. A violation on every attempt still fails; an intermittent one would
// only show as flaky.
test.describe(() => {
  test.describe.configure({ retries: 1 });

  test('RF-A18: no CSP violations on /es/dev/arena while loading and 5 s of playback', async ({
    page,
  }) => {
    await collectViolations(page);
    await page.goto('/es/dev/arena');
    const stage = page.getByTestId('fight-stage');
    await expect(stage).toHaveAttribute('data-renderer-loaded', '', { timeout: 15_000 });
    await expect(stage).toHaveAttribute('data-state', /^(ready|error)$/, { timeout: 15_000 });
    if ((await stage.getAttribute('data-state')) === 'error') {
      await expect(stage).toHaveAttribute('data-error', 'webgl_unavailable');
      test.info().annotations.push({ type: 'limited', description: 'no WebGL in this engine' });
    } else {
      // TA-20: the arena loads paused; playback starts with Play.
      await waitForHydration(page);
      await page.getByTestId('arena-toggle').click();
      await expect(stage).toHaveAttribute('data-state', 'playing');
      await page.waitForTimeout(5000);
      await expect(stage).toHaveAttribute('data-state', 'playing');
    }
    expect(await violations(page)).toEqual([]);
  });
});

// ADR-0012 B4.6 (step 14 of 008 plan-d): the login and the account load under the amended CSP
// without a violation, in Chromium and WebKit.
authTest('B4.6: no CSP violations on /login and /account', async ({ page, identity }) => {
  await collectViolations(page);
  await page.goto('/es/login');
  await waitForHydration(page);
  expect(await violations(page), '/es/login').toEqual([]);
  await authLogin(page, identity, 'en', '/en/account');
  if (page.url().includes('/onboarding')) await page.getByTestId('onboarding-skip').click();
  await page.waitForURL('/en/account');
  await waitForHydration(page);
  expect(await violations(page), '/en/account').toEqual([]);
});
