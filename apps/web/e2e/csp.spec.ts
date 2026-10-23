import { expect, test, type Page } from '@playwright/test';

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
  await page.getByTestId('dropdown-trigger').click();
  await expect(page.getByTestId('dropdown-content')).toBeVisible();
  await page.keyboard.press('Escape');
  await page.getByTestId('dialog-trigger').click();
  await expect(page.getByTestId('dialog-content')).toBeVisible();
  await page.waitForTimeout(300);
  expect(await violations(page)).toEqual([]);
});
