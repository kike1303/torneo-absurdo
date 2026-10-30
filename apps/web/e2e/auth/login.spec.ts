import { waitForHydration } from '../support/hydration.js';
import {
  canaryName,
  expect,
  identityFor,
  login,
  me,
  newContext,
  press,
  test,
  text,
  type Locale,
} from './fixtures.js';

// 008 RF-D8 (plan-d decision 11): login with `dev`, onboarding with a rename, reload with the
// session, logout, login again with the same identity, in en and es; Chromium and WebKit with
// mobile emulation (375 px and touch).

for (const locale of ['es', 'en'] as const satisfies Locale[]) {
  test(`${locale}: login, rename, reload, logout and login again`, async ({ page, identity }) => {
    const coins = new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(100);
    const chosen = canaryName();

    await test.step('1. the three buttons, login with dev and the onboarding', async () => {
      await page.goto(`/${locale}/login`);
      for (const provider of ['google', 'discord', 'dev']) {
        await expect(page.getByTestId(`login-provider-${provider}`)).toBeVisible();
      }
      await login(page, identity, locale);
      await expect(page).toHaveURL(new RegExp(`/${locale}/onboarding\\?next=`));
      const generated = await page.getByTestId('onboarding-alias').textContent();
      expect(generated?.trim().length).toBeGreaterThanOrEqual(3);
      await page.getByLabel(text(locale, 'display_name_label')).fill(chosen);
      await press(page.getByTestId('display-name-save'));
      await page.waitForURL(`/${locale}/`);
      await expect(page.getByTestId('header-user')).toContainText(chosen);
      await expect(page.getByTestId('header-balance')).toHaveText(
        text(locale, 'coins_amount', { amount: coins }),
      );
    });

    await test.step('2. a reload keeps the session, private and not stored', async () => {
      const res = await page.reload();
      expect(res?.headers()['cache-control']).toBe('private, no-store');
      const vary = (res?.headers()['vary'] ?? '').split(',').map((v) => v.trim().toLowerCase());
      expect(vary).toContain('cookie');
      await expect(page.getByTestId('header-user')).toContainText(chosen);
    });

    await test.step('3. sign out: the header says sign in, the account goes to login', async () => {
      await page.goto(`/${locale}/account`);
      await waitForHydration(page);
      await press(page.getByTestId('account-logout'));
      await page.waitForURL((url) => !url.pathname.endsWith('/account'));
      await expect(page.getByTestId('header-sign-in')).toBeVisible();
      await page.goto(`/${locale}/account`);
      await expect(page).toHaveURL(new RegExp(`/${locale}/login`));
    });

    await test.step('4. login again: same name, no onboarding, still 100 coins', async () => {
      await login(page, identity, locale);
      await expect(page).toHaveURL(`/${locale}/`);
      await expect(page.getByTestId('header-user')).toContainText(chosen);
      await expect(page.getByTestId('header-balance')).toHaveText(
        text(locale, 'coins_amount', { amount: coins }),
      );
      expect((await me(page)).balance).toBe(100);
    });
  });
}

test('5. a name taken by another user shows its text, tied to the field', async ({
  page,
  browser,
}, info) => {
  const taken = canaryName();
  const first = await newContext(browser, info, 'first');
  const other = await first.newPage();
  await login(other, identityFor(info, 'first'), 'en');
  await other.getByLabel(text('en', 'display_name_label')).fill(taken);
  await press(other.getByTestId('display-name-save'));
  await other.waitForURL('/en/');
  await first.close();

  await login(page, identityFor(info, 'second'), 'en');
  const field = page.getByLabel(text('en', 'display_name_label'));
  await field.fill(taken);
  await press(page.getByTestId('display-name-save'));
  const error = page.getByTestId('display-name-error');
  await expect(error).toHaveText(text('en', 'error_profile_display_name_taken'));
  const id = await error.getAttribute('id');
  expect(id).toBeTruthy();
  await expect(field).toHaveAttribute('aria-describedby', new RegExp(`(^| )${id}( |$)`));
  await expect(field).toHaveAttribute('aria-invalid', 'true');
  await expect(field).toBeFocused();
});

test('6. "Skip" is not saved: the onboarding comes back on the next login', async ({
  page,
  identity,
}) => {
  await login(page, identity, 'es');
  await expect(page).toHaveURL(/\/es\/onboarding\?next=/);
  const generated = (await page.getByTestId('onboarding-alias').textContent())?.trim() ?? '';
  await press(page.getByTestId('onboarding-skip'));
  await page.waitForURL('/es/');
  await expect(page.getByTestId('header-user')).toContainText(generated);

  await page.goto('/es/account');
  await waitForHydration(page);
  await press(page.getByTestId('account-logout'));
  await expect(page.getByTestId('header-sign-in')).toBeVisible();

  await login(page, identity, 'es');
  await expect(page).toHaveURL(/\/es\/onboarding\?next=/);
  await expect(page.getByTestId('onboarding-alias')).toHaveText(generated);
});
