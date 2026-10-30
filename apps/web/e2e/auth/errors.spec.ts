import { randomBytes } from 'node:crypto';
import type { Page } from '@playwright/test';
import { waitForHydration } from '../support/hydration.js';
import { EGRESS_LOG, addDynamicCanary, expectEgress, fileLines } from '../support/logs.js';
import { PUBLIC_ORIGIN } from '../support/topology.js';
import { expect, login, press, test, text } from './fixtures.js';

// 008 RF-D6, the T4 case of the 2026-10-28 amendment, the 2026-10-30 amendment of RF-B5 and
// open question 1 of plan-d (decision 11). `page.route` never sees the destination of a 303, so
// the OAuth steps are first-URL navigations: the `start` and the `authorize` of `dev` by
// `page.request` (it shares the context's cookies, the flow cookie included), then `page.goto`.

const canary = (kind: string) => {
  const value = `canary-${kind}-${randomBytes(4).toString('hex')}`;
  addDynamicCanary(value);
  return value;
};

/** `start` and `authorize` of `dev`: the callback URL with its `code` and `state`. */
async function callbackUrl(page: Page, identity: string): Promise<URL> {
  addDynamicCanary(identity);
  const start = await page.request.post('/api/auth/dev/start', {
    form: { dev_identity: identity, next: '/en/welcome' },
    headers: { origin: PUBLIC_ORIGIN },
    maxRedirects: 0,
  });
  expect(start.status()).toBe(303);
  const authorize = await page.request.get(start.headers().location ?? '', { maxRedirects: 0 });
  expect(authorize.status()).toBe(303);
  return new URL(authorize.headers().location ?? '', PUBLIC_ORIGIN);
}

/** Everything a page could reflect: the HTML and every attribute of every element. */
async function reflected(page: Page): Promise<string> {
  const html = await page.content();
  const attributes = await page.evaluate(() =>
    [...document.querySelectorAll('*')]
      .flatMap((el) => [...el.attributes].map((a) => `${a.name}=${a.value}`))
      .join('\n'),
  );
  return `${html}\n${attributes}`;
}

test('1. an invalid state ends in /login with the translated text', async ({ page, identity }) => {
  const url = await callbackUrl(page, identity);
  url.searchParams.set('state', `${url.searchParams.get('state')}x`);
  await page.goto(url.toString());
  await expect(page).toHaveURL(/\/en\/login\?error=auth\.invalid_state$/);
  await expect(page.getByTestId('login-error')).toHaveText(text('en', 'error_auth_invalid_state'));
});

test('2. access_denied of the provider: its text, never error_description', async ({
  page,
  identity,
}) => {
  const url = await callbackUrl(page, identity);
  const description = canary('description');
  const denied = new URL('/api/auth/dev/callback', PUBLIC_ORIGIN);
  denied.searchParams.set('error', 'access_denied');
  denied.searchParams.set('error_description', description);
  denied.searchParams.set('state', url.searchParams.get('state') ?? '');
  await page.goto(denied.toString());
  await expect(page).toHaveURL(/\/en\/login\?error=auth\.provider_error$/);
  await expect(page.getByTestId('login-error')).toHaveText(text('en', 'error_auth_provider_error'));
  expect(await reflected(page)).not.toContain(description);
});

test('3. an unknown ?error= is never reflected (Location, HTML, attributes, logs)', async ({
  page,
}) => {
  const value = canary('error');
  for (const raw of [
    value,
    `<script>${value}</script>`,
    '__proto__',
    'auth.csrf_origin',
    `%0d%0a${value}`,
  ]) {
    const path = `/es/login?error=${raw.startsWith('%') ? raw : encodeURIComponent(raw)}`;
    const res = await page.request.get(path, { maxRedirects: 0 });
    expect(res.status(), raw).toBe(303);
    const location = res.headers().location ?? '';
    expect(location, raw).toBe('/es/login?error=');
    await page.goto(path);
    await expect(page).toHaveURL('/es/login?error=');
    await expect(page.getByTestId('login-error')).toHaveText(text('es', 'error_generic'));
    const all = await reflected(page);
    expect(all, raw).not.toContain(value);
    expect(all, raw).not.toContain('csrf_origin');
  }
});

test('4. with a session, the error and a link to the account', async ({ page, identity }) => {
  await login(page, identity, 'es');
  await page.goto('/es/login?error=auth.already_linked');
  await expect(page.getByTestId('login-error')).toHaveText(text('es', 'error_auth_already_linked'));
  await expect(page.getByTestId('login-account-link')).toHaveAttribute('href', '/es/account');
});

/** Collects `securitypolicyviolation` events of the page. */
async function watchViolations(page: Page): Promise<() => Promise<string[]>> {
  await page.evaluate(() => {
    const w = window as unknown as { __violations: string[] };
    w.__violations = [];
    document.addEventListener('securitypolicyviolation', (e) =>
      w.__violations.push(e.effectiveDirective),
    );
  });
  return () => page.evaluate(() => (window as unknown as { __violations: string[] }).__violations);
}

// Fixme until step 14 of plan-d: the 2026-10-30 amendment of ADR-0012 (B4.6) widens
// form-action to the two authorize endpoints. Run once on B1 (2026-10-30): Chromium, Chromium
// mobile and WebKit mobile all block the 303 to the provider (no CONNECT reaches the proxy).
test.fixme('5. open question 1: the Google and Discord buttons reach their provider', async ({
  page,
}) => {
  for (const [provider, host] of [
    ['google', 'accounts.google.com:443'],
    ['discord', 'discord.com:443'],
  ] as const) {
    expectEgress(host);
    await page.goto('/en/login');
    await waitForHydration(page);
    const violations = await watchViolations(page);
    const before = fileLines(EGRESS_LOG).length;
    await press(page.getByTestId(`login-provider-${provider}`));
    await expect.poll(() => fileLines(EGRESS_LOG).slice(before)).toContain(host);
    expect(await violations().catch(() => [])).toEqual([]);
  }
});

test('6. the 2026-10-30 amendment: an exhausted quota ends in /login, never in JSON', async ({
  page,
  identity,
}) => {
  // Its own x-e2e-client (the fixture), so its own /64: no other test shares this bucket.
  addDynamicCanary(identity);
  let exhausted = false;
  for (let i = 0; i < 12 && !exhausted; i++) {
    const res = await page.request.post('/api/auth/dev/start', {
      form: { dev_identity: identity, next: '/en/welcome' },
      headers: { origin: PUBLIC_ORIGIN },
      maxRedirects: 0,
    });
    expect(res.status()).toBe(303);
    const location = res.headers().location ?? '';
    if (location === '/login?error=auth.try_again') {
      expect(res.headers()['retry-after']).toBeUndefined();
      exhausted = true;
    } else {
      expect(location).toContain('/api/auth/dev/oauth/authorize');
    }
  }
  expect(exhausted).toBe(true);
  await page.goto('/en/login');
  await waitForHydration(page);
  await page.getByLabel(text('en', 'login_dev_identity_label')).fill(identity);
  await press(page.getByTestId('login-provider-dev'));
  await expect(page).toHaveURL(/\/en\/login\?error=auth\.try_again$/);
  await expect(page.getByTestId('login-error')).toHaveText(text('en', 'error_auth_try_again'));
  expect(await page.content()).toContain('<html');
});

test('7. B4.6: a form injected towards a Discord webhook is blocked by form-action', async ({
  page,
}) => {
  await page.goto('/en/login');
  await waitForHydration(page);
  const violations = await watchViolations(page);
  const before = fileLines(EGRESS_LOG).length;
  await page.evaluate(() => {
    const form = document.createElement('form');
    form.method = 'post';
    form.action = 'https://discord.com/api/webhooks/x';
    document.body.append(form);
    form.submit();
  });
  await expect.poll(violations).toContain('form-action');
  await page.waitForTimeout(500);
  expect(fileLines(EGRESS_LOG).slice(before)).toEqual([]);
});
