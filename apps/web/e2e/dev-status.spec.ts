import { expect, test, type Page } from '@playwright/test';

// Optional e2e against `just dev` (playwright.dev.config.ts), outside `just e2e` and CI: `node
// build` has no proxy to /api and /ws (spec, "Áreas de preocupación" 4). Covers RF-B7 and RF-B17.

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

for (const locale of ['es', 'en'] as const) {
  test(`/${locale}/dev/status: server ok and an echo round trip`, async ({ page }) => {
    await collectViolations(page);
    await page.goto(`/${locale}/dev/status`);
    const server = page.getByTestId('server-status');
    await expect(server.locator('[data-state="ok"]')).toBeVisible();
    await expect(server).toContainText('dev');

    const echo = page.getByTestId('echo-status');
    await expect(echo.locator('[data-state="connected"]')).toBeVisible();
    const payload = `e2e-${Date.now()}`;
    await page.locator('#echo-input').fill(payload);
    await page.getByRole('button', { name: locale === 'es' ? 'Enviar' : 'Send' }).click();
    await expect(page.getByTestId('echo-payload')).toHaveText(payload);
    await expect(echo.locator('[data-state="received"]')).toBeVisible();

    expect(await violations(page)).toEqual([]);
  });
}
