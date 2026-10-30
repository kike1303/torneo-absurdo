import { createHash, randomBytes } from 'node:crypto';
import { readFileSync } from 'node:fs';
import {
  test as base,
  expect,
  type Browser,
  type BrowserContext,
  type Locator,
  type Page,
  type TestInfo,
} from '@playwright/test';
import { waitForHydration } from '../support/hydration.js';
import { addDynamicCanary } from '../support/logs.js';
import { EGRESS_PROXY, PUBLIC_ORIGIN } from '../support/topology.js';

// The auth e2e (008 RF-D8, plan-d decision 11). Every test gets its own `dev_identity` and its
// own `x-e2e-client` (so its own /64 and its own AUTH_IP bucket at the edge), derived from the
// project and the title: no two tests share a user or a quota, and the database is new per run.

export type Locale = 'en' | 'es';

type Messages = Record<string, string>;
const load = (locale: Locale): Messages =>
  JSON.parse(readFileSync(new URL(`../../messages/${locale}.json`, import.meta.url), 'utf8'));
const MESSAGES: Record<Locale, Messages> = { en: load('en'), es: load('es') };

/** The text of a message, with `{name}` parameters filled. */
export function text(locale: Locale, key: string, params: Record<string, string> = {}): string {
  const template = MESSAGES[locale][key];
  if (template === undefined) throw new Error(`no message ${key}`);
  return template.replace(/\{(\w+)\}/g, (_, name: string) => params[name] ?? `{${name}}`);
}

function hash(value: string, length: number): string {
  return createHash('sha256').update(value).digest('hex').slice(0, length);
}

/** `e2e-<12 hex>`: a valid `dev_identity` (`^[a-z0-9_-]{1,32}$`), unique per test and `tag`. */
export function identityFor(info: TestInfo, tag = ''): string {
  return `e2e-${hash(`${info.project.name}|${info.titlePath.join('|')}|${tag}`, 12)}`;
}

/** The `x-e2e-client` of a test: the edge turns it into the test's own /64. */
export function clientFor(info: TestInfo, tag = ''): string {
  return `e2e-${hash(`client|${info.project.name}|${info.titlePath.join('|')}|${tag}`, 16)}`;
}

/** A name for a rename: 3 to 24 characters, unique per run, declared as a canary. */
export function canaryName(): string {
  const name = `Canario${randomBytes(4).toString('hex')}`;
  addDynamicCanary(name);
  return name;
}

/** A tap where the project emulates touch (RF-D8: "toque, no clic"), a click elsewhere. */
export async function press(locator: Locator): Promise<void> {
  if (base.info().project.use.hasTouch) await locator.tap();
  else await locator.click();
}

/**
 * Logs in with the `dev` provider from `/<locale>/login` (with `next` if given) and waits until
 * the browser leaves the login flow. Declares the identity and the session token as canaries.
 */
export async function login(
  page: Page,
  identity: string,
  locale: Locale,
  next?: string,
): Promise<void> {
  addDynamicCanary(identity);
  const query = next ? `?next=${encodeURIComponent(next)}` : '';
  await page.goto(`/${locale}/login${query}`);
  await waitForHydration(page);
  await page.getByLabel(text(locale, 'login_dev_identity_label')).fill(identity);
  await press(page.getByTestId('login-provider-dev'));
  // Out of the flow: not /api/**, not /welcome, and not the login page again unless with an error.
  const loginPath = `/${locale}/login`;
  await page.waitForURL(
    (url) =>
      !url.pathname.startsWith('/api/') &&
      !url.pathname.endsWith('/welcome') &&
      (url.pathname !== loginPath || url.searchParams.has('error')),
  );
  await waitForHydration(page);
  await declareSessionToken(page.context());
}

/** The value of the session cookie, so the teardown looks for it in every log. */
export async function declareSessionToken(context: BrowserContext): Promise<string | undefined> {
  const cookie = (await context.cookies(PUBLIC_ORIGIN)).find((c) => c.name === 'ta_dev_session');
  if (cookie?.value) addDynamicCanary(cookie.value);
  return cookie?.value;
}

/**
 * Another context of the same browser with the project's emulation and the network guard (a
 * manual `newContext` does not get the config's `use.proxy`), and its own `x-e2e-client`.
 */
export async function newContext(
  browser: Browser,
  info: TestInfo,
  tag: string,
): Promise<BrowserContext> {
  const use = info.project.use;
  return browser.newContext({
    baseURL: PUBLIC_ORIGIN,
    proxy: EGRESS_PROXY,
    viewport: use.viewport,
    userAgent: use.userAgent,
    deviceScaleFactor: use.deviceScaleFactor,
    isMobile: use.isMobile,
    hasTouch: use.hasTouch,
    extraHTTPHeaders: { 'x-e2e-client': clientFor(info, tag) },
  });
}

/** `Me` from `GET /api/me` with the cookies of `page`. */
export async function me(page: Page): Promise<Record<string, unknown>> {
  const res = await page.request.get('/api/me');
  expect(res.status()).toBe(200);
  return (await res.json()) as Record<string, unknown>;
}

// Playwright reads the fixtures a fixture needs from its first parameter, which must be an
// object pattern even when empty.
/* eslint-disable no-empty-pattern */
export const test = base.extend<{ identity: string }>({
  extraHTTPHeaders: async ({}, use, info) => {
    await use({ 'x-e2e-client': clientFor(info) });
  },
  identity: async ({}, use, info) => {
    await use(identityFor(info));
  },
});
/* eslint-enable no-empty-pattern */

export { expect };
