import { randomBytes } from 'node:crypto';
import { ageSession, seedOauthAccount } from '../support/db.mjs';
import { waitForHydration } from '../support/hydration.js';
import { addDynamicCanary } from '../support/logs.js';
import { PUBLIC_ORIGIN } from '../support/topology.js';
import {
  canaryName,
  expect,
  identityFor,
  login,
  me,
  newContext,
  press,
  reauth,
  reflected,
  test,
  text,
} from './fixtures.js';

// 008 RF-D4 and RF-D5 (plan-d decision 11) and 009 RF20 to RF24 (decision 14): the account screen.
// The AUTH_IP burst is 5, a login spends 2 and a reauthentication 2: no context of a test spends
// more than 4, and a third round trip goes in a `newContext` with its own bucket.

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

test('1. unlink: the panel, the round trip, and then the last one cannot go', async ({
  page,
  identity,
}) => {
  await toAccount(page, identity, 'es');
  // `dev` is the only provider without network; Google is linked straight in the database, and
  // that link leaves the authentication of this session without validity.
  await seedOauthAccount(identity, 'google');
  await page.reload();
  await waitForHydration(page);
  // Discord is still free to link, so its panel is there from the start.
  await expect(page.getByTestId('reauth-panel-link')).toBeVisible();
  await expect(page.getByTestId('unlink-google')).toBeEnabled();
  await expect(page.getByTestId('unlink-dev')).toBeEnabled();

  await press(page.getByTestId('unlink-google'));
  const panel = page.getByTestId('reauth-panel-google');
  await expect(panel).toBeVisible();
  await expect(
    page.getByTestId('provider-row-google').getByTestId('reauth-panel-google'),
  ).toBeVisible();
  await expect(panel.getByTestId('reauth-title')).toHaveText(text('es', 'account_reauth_title'));
  await expect(panel.getByTestId('reauth-title')).toBeFocused();
  await expect(panel).toContainText(text('es', 'account_reauth_text'));
  await expect(
    panel.getByRole('button', {
      name: text('es', 'account_reauth_with', { provider: text('es', 'provider_name_dev') }),
    }),
  ).toBeVisible();
  // One panel at a time, and it replaces the error of its row.
  await expect(page.getByTestId('reauth-panel-link')).toHaveCount(0);
  await expect(page.getByLabel(text('es', 'login_dev_identity_label'))).toHaveCount(1);
  await expect(page.getByTestId('provider-error-google')).toHaveCount(0);

  await reauth(page, identity, 'reauth-panel-google');
  await expect(page.getByTestId('account-status')).toHaveText(text('es', 'account_reauth_done'));
  // Nothing happens by itself on the way back: no GET mutates (RF23).
  expect(((await me(page)).providers as string[]).sort()).toEqual(['dev', 'google']);

  await press(page.getByTestId('unlink-google'));
  await expect(page.getByTestId('account-status')).toHaveText(text('es', 'account_unlinked'));
  // The unlink used up the confirmation: linking needs another one.
  await expect(page.getByTestId('reauth-panel-link')).toBeVisible();
  await expect(page.getByTestId('link-google')).toHaveCount(0);
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

test('2. an aged session: the panel, the round trip, and the unlink passes', async ({
  page,
  browser,
  identity,
}, info) => {
  await toAccount(page, identity, 'en');
  await seedOauthAccount(identity, 'google');

  // A second login is valid again, but a third round trip does not fit in one bucket.
  const aged = await newContext(browser, info, 'aged');
  const second = await aged.newPage();
  await toAccount(second, identity, 'en');
  await ageSession(identity, 11);
  await second.reload();
  await waitForHydration(second);

  // A submit that leaves the focus on another field (Enter, or a browser that does not blur the
  // pressed button): only the effect of the panel moves it, Svelte's `autofocus` needs `body`.
  const field = second.locator('input[name="display_name"]');
  await field.focus();
  await second
    .getByTestId('unlink-google')
    .evaluate((button: HTMLButtonElement) => button.form?.requestSubmit(button));
  const panel = second.getByTestId('reauth-panel-google');
  await expect(panel.getByTestId('reauth-title')).toHaveText(text('en', 'account_reauth_title'));
  await expect(panel.getByTestId('reauth-title')).toBeFocused();
  await reauth(second, identity, 'reauth-panel-google');
  await expect(second.getByTestId('account-status')).toHaveText(text('en', 'account_reauth_done'));

  await press(second.getByTestId('unlink-google'));
  await expect(second.getByTestId('account-status')).toHaveText(text('en', 'account_unlinked'));
  expect(((await me(second)).providers as string[]).sort()).toEqual(['dev']);
  await aged.close();
});

test('3. signing in with one and removing the other needs no panel', async ({
  page,
  browser,
  identity,
}, info) => {
  await toAccount(page, identity, 'es');
  await seedOauthAccount(identity, 'google');

  const again = await newContext(browser, info, 'again');
  const second = await again.newPage();
  await toAccount(second, identity, 'es');
  await press(second.getByTestId('unlink-google'));
  await expect(second.getByTestId('account-status')).toHaveText(text('es', 'account_unlinked'));
  await expect(second.getByTestId('reauth-panel-google')).toHaveCount(0);
  await again.close();
});

test('4. link: without a confirmation the panel takes the place of the button', async ({
  page,
  identity,
}) => {
  await toAccount(page, identity, 'es');
  // A fresh login can link right away.
  await expect(page.getByTestId('link-google')).toBeVisible();
  await expect(page.getByTestId('reauth-panel-link')).toHaveCount(0);

  await ageSession(identity, 11);
  await page.reload();
  await waitForHydration(page);
  await expect(page.getByTestId('reauth-panel-link')).toBeVisible();
  await expect(page.getByTestId('link-google')).toHaveCount(0);
  // The panel appeared by itself, so it does not take the focus.
  await expect(page.getByTestId('reauth-title')).not.toBeFocused();

  await reauth(page, identity, 'reauth-panel-link');
  await expect(page.getByTestId('account-status')).toHaveText(text('es', 'account_reauth_done'));
  // Never pressed: the egress proxy would cut the provider.
  await expect(page.getByTestId('link-google')).toBeVisible();
  await expect(page.getByTestId('reauth-panel-link')).toHaveCount(0);

  // With `enhance`, Kit lands on the URL of the action without its key: ?reauth=1 goes away.
  await page.locator('input[name="display_name"]').fill(canaryName());
  await press(page.getByTestId('display-name-save'));
  await expect(page.getByTestId('account-status')).toHaveText(text('es', 'account_renamed'));
  await expect(page).toHaveURL('/es/account');
});

test('4b. a confirmation that expires on the way back opens the panel again, with the focus', async ({
  page,
  identity,
}) => {
  await toAccount(page, identity, 'es');
  await seedOauthAccount(identity, 'google');
  await page.reload();
  await waitForHydration(page);
  await reauth(page, identity, 'reauth-panel-link');

  // This 403 leaves from ?reauth=1, so its result arrives through a navigation of Kit.
  await ageSession(identity, 11);
  await press(page.getByTestId('unlink-google'));
  const panel = page.getByTestId('reauth-panel-google');
  await expect(panel.getByTestId('reauth-title')).toBeFocused();
  await expect(page).toHaveURL('/es/account');
  await expect(page.getByTestId('account-status')).toHaveText('');
  expect(((await me(page)).providers as string[]).sort()).toEqual(['dev', 'google']);
});

for (const locale of ['es', 'en'] as const) {
  test(`5. another identity (${locale}): auth.reauth_mismatch and the way back`, async ({
    page,
    identity,
  }, info) => {
    // The callback lands on the unprefixed /login, which goes by the cookie: the language of this
    // browser stays on `locale` whatever its Accept-Language says.
    await page.context().addCookies([{ name: 'locale', value: locale, url: PUBLIC_ORIGIN }]);
    await toAccount(page, identity, locale);
    const alias = await page.getByTestId('account-alias').textContent();
    await ageSession(identity, 11);
    await page.reload();
    await waitForHydration(page);

    const other = identityFor(info, 'other');
    addDynamicCanary(other);
    const panel = page.getByTestId('reauth-panel-link');
    await panel.getByLabel(text(locale, 'login_dev_identity_label')).fill(other);
    await press(panel.getByTestId('reauth-provider-dev'));

    await expect(page).toHaveURL(new RegExp(`/${locale}/login\\?error=auth\\.reauth_mismatch$`));
    await expect(page.getByTestId('login-error')).toHaveText(
      text(locale, 'error_auth_reauth_mismatch'),
    );
    // TA-31: the session is still there, so nothing asks to sign in.
    await expect(page.getByTestId('login-title')).toHaveText(text(locale, 'login_signed_in_title'));
    await expect(page.getByText(text(locale, 'login_lead'))).toHaveCount(0);
    const back = page.getByTestId('login-account-link');
    await expect(back).toHaveText(text(locale, 'login_back_to_account'));
    await waitForHydration(page);
    await press(back);

    // The same session: nothing was rotated, and no user was created.
    await page.waitForURL(`/${locale}/account`);
    await expect(page.getByTestId('account-alias')).toHaveText(alias ?? '');
    await expect(page.getByTestId('reauth-panel-link')).toBeVisible();
  });
}

test('6. ?reauth=<value> never reaches the page (Location, HTML, attributes, logs)', async ({
  page,
  identity,
}) => {
  await toAccount(page, identity, 'es');
  const value = `canary-reauth-${randomBytes(4).toString('hex')}`;
  addDynamicCanary(value);
  const path = `/es/account?reauth=${value}&x=1`;

  const res = await page.request.get(path, { maxRedirects: 0 });
  expect(res.status()).toBe(303);
  expect(res.headers().location).toBe('/es/account?reauth=1');

  await page.goto(path);
  await expect(page).toHaveURL('/es/account?reauth=1');
  await expect(page.getByTestId('account-alias')).toBeVisible();
  const all = await reflected(page);
  expect(all).not.toContain(value);
  // The locale switcher copies `search`: only the fixed value is left to copy.
  await expect(page.locator('#locale-form-en input[name="to"]')).toHaveValue(
    '/en/account?reauth=1',
  );
});

test('7. theme and language with a session go to the profile and back on login', async ({
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

test('8. "sign out everywhere" closes the session of another browser', async ({
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
