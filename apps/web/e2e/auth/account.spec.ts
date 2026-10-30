import { seedOauthAccount } from '../support/db.mjs';
import { waitForHydration } from '../support/hydration.js';
import { PUBLIC_ORIGIN } from '../support/topology.js';
import { expect, login, me, newContext, press, test, text } from './fixtures.js';

// 008 RF-D4 and RF-D5 (plan-d decision 11): the account screen.

/** Logs in and goes to the account (skipping the onboarding of a generated name). */
async function toAccount(
  page: import('@playwright/test').Page,
  identity: string,
  locale: 'es' | 'en',
) {
  await login(page, identity, locale, `/${locale}/account`);
  if (page.url().includes('/onboarding')) {
    await press(page.getByTestId('onboarding-skip'));
  }
  await page.waitForURL(`/${locale}/account`);
  await waitForHydration(page);
}

test('1. unlink: two providers, then the last one cannot go', async ({ page, identity }) => {
  await toAccount(page, identity, 'es');
  // `dev` is the only provider without network; Google is linked straight in the database.
  await seedOauthAccount(identity, 'google');
  await page.reload();
  await waitForHydration(page);
  await expect(page.getByTestId('unlink-google')).toBeEnabled();
  await expect(page.getByTestId('unlink-dev')).toBeEnabled();

  await press(page.getByTestId('unlink-google'));
  await expect(page.getByTestId('account-status')).toHaveText(text('es', 'account_unlinked'));
  await expect(page.getByTestId('link-google')).toBeVisible();
  await expect(page.getByTestId('unlink-dev')).toBeDisabled();
  await expect(page.getByTestId('provider-hint-dev')).toHaveText(
    text('es', 'account_last_provider_hint'),
  );

  // A forced submit (the server is the source of truth): its text, never raw JSON.
  await page.getByTestId('unlink-dev').evaluate((button) => button.removeAttribute('disabled'));
  await press(page.getByTestId('unlink-dev'));
  await expect(page.getByTestId('provider-error-dev')).toHaveText(
    text('es', 'error_auth_last_provider'),
  );
  expect(await page.content()).not.toContain('{"error"');
  expect(((await me(page)).providers as string[]).sort()).toEqual(['dev']);
});

test('2. theme and language with a session go to the profile and back on login', async ({
  page,
  browser,
  identity,
}, info) => {
  await toAccount(page, identity, 'es');

  await press(page.getByTestId('theme-switcher'));
  await press(page.getByTestId('theme-option-light'));
  await expect(page.locator('html')).toHaveAttribute('data-theme', 'light');
  await expect.poll(async () => (await me(page)).theme).toBe('light');
  const cookies = async () =>
    Object.fromEntries((await page.context().cookies(PUBLIC_ORIGIN)).map((c) => [c.name, c.value]));
  expect((await cookies()).theme).toBe('light');

  await press(page.getByTestId('locale-switcher'));
  await press(page.getByTestId('locale-option-en'));
  await page.waitForURL('/en/account');
  await expect(page.locator('html')).toHaveAttribute('lang', 'en');
  expect((await me(page)).locale).toBe('en');
  expect((await cookies()).locale).toBe('en');

  // Another browser with the same identity gets the profile's cookies at /welcome, and the
  // destination keeps the language of its own prefix (plan-d decision 5).
  const other = await newContext(browser, info, 'other');
  const second = await other.newPage();
  await login(second, identity, 'es', '/es/');
  // The alias is still the generated one, so the onboarding comes back (deviation D3).
  await expect(second).toHaveURL(/\/es\/onboarding\?next=%2Fes%2F$/);
  await press(second.getByTestId('onboarding-skip'));
  await expect(second).toHaveURL('/es/');
  await expect(second.locator('html')).toHaveAttribute('lang', 'es');
  await expect(second.locator('html')).toHaveAttribute('data-theme', 'light');
  const theirs = Object.fromEntries(
    (await other.cookies(PUBLIC_ORIGIN)).map((c) => [c.name, c.value]),
  );
  expect(theirs).toMatchObject({ theme: 'light', locale: 'en' });
  // An unprefixed route now follows the profile.
  await second.goto('/');
  await expect(second).toHaveURL('/en/');
  await other.close();
});

test('3. "sign out everywhere" closes the session of another browser', async ({
  page,
  browser,
  identity,
}, info) => {
  await toAccount(page, identity, 'en');
  const other = await newContext(browser, info, 'other');
  const second = await other.newPage();
  await login(second, identity, 'en', '/en/');
  await expect(second.getByTestId('header-user')).toBeVisible();

  await press(page.getByTestId('account-logout-all'));
  await expect(page.getByTestId('header-sign-in')).toBeVisible();

  await second.reload();
  await expect(second.getByTestId('header-sign-in')).toBeVisible();
  await other.close();
});
