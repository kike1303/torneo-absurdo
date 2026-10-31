import { expect, test, type Page } from '@playwright/test';
import { login as authLogin, press, test as authTest } from './auth/fixtures.js';
import { seedOauthAccount } from './support/db.mjs';
import { waitForHydration } from './support/hydration.js';
import { PUBLIC_ORIGIN } from './support/topology.js';

// Test 8 and the 375 px RNF: artifacts for review (no pixel comparison), plus no horizontal scroll.
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;
const THEMES = ['light', 'dark'] as const;
const DIR = 'test-results/screenshots';
/** Just after the first crit and knockdown of log-ko-comeback (at 29 415 ms). */
const LOG_SHOT_T = 29_600;

for (const size of SIZES) {
  for (const theme of THEMES) {
    test(`catalog ${size.name} ${theme}`, async ({ page }) => {
      await page.setViewportSize(size);
      await page.goto(`/es/dev/components?theme=${theme}`);
      await page.screenshot({ path: `${DIR}/catalog-${size.name}-${theme}.png`, fullPage: true });
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
    });

    for (const [name, path] of [
      ['home-es', '/es/'],
      ['home-en', '/en/'],
      ['404-es', '/es/no-existe'],
    ] as const) {
      test(`${name} ${size.name} ${theme}`, async ({ page }) => {
        await page
          .context()
          .addCookies([{ name: 'theme', value: theme, url: 'http://localhost:4173' }]);
        await page.setViewportSize(size);
        await page.goto(path);
        await page.screenshot({ path: `${DIR}/${name}-${size.name}-${theme}.png`, fullPage: true });
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow).toBeLessThanOrEqual(0);
      });
    }
  }
}

test('header: both selectors fit in one row at 375 px', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto('/es/');
  const locale = await page.getByTestId('locale-switcher').boundingBox();
  const theme = await page.getByTestId('theme-switcher').boundingBox();
  expect(locale && theme).toBeTruthy();
  expect(Math.abs((locale?.y ?? 0) - (theme?.y ?? 1))).toBeLessThan(2);
  expect((theme?.x ?? 0) + (theme?.width ?? 0)).toBeLessThanOrEqual(375);
});

// 002 RF-C3: the arena with an engine log (ko-comeback), paused just after its first knockdown.
for (const size of SIZES) {
  for (const theme of THEMES) {
    for (const lang of ['es', 'en'] as const) {
      test(`arena-log ${lang} ${size.name} ${theme}`, async ({ page }) => {
        await page
          .context()
          .addCookies([{ name: 'theme', value: theme, url: 'http://localhost:4173' }]);
        await page.setViewportSize(size);
        await page.goto(`/${lang}/dev/arena?log=ko-comeback&t=${LOG_SHOT_T}`);
        await expect(page.getByTestId('fight-stage')).toHaveAttribute('data-state', 'ready', {
          timeout: 15_000,
        });
        await page.waitForTimeout(250);
        await page.screenshot({
          path: `${DIR}/arena-log-${lang}-${size.name}-${theme}.png`,
          fullPage: true,
        });
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow).toBeLessThanOrEqual(0);
      });
    }
  }
}

// 005 RF18: the engine logs that end by time, at 88 500 ms (the time banner, and the winner's pose
// in a decision), and at the end on mobile (the result text has to wrap, never overflow).
const TIME_SHOT_T = 88_500;
const TIME_LOGS = ['decision-a', 'decision-b', 'draw'] as const;

for (const log of TIME_LOGS) {
  for (const lang of ['es', 'en'] as const) {
    for (const size of SIZES) {
      for (const theme of THEMES) {
        test(`arena-time ${log} ${lang} ${size.name} ${theme}`, async ({ page }) => {
          await page
            .context()
            .addCookies([{ name: 'theme', value: theme, url: 'http://localhost:4173' }]);
          await page.setViewportSize(size);
          await page.goto(`/${lang}/dev/arena?log=${log}&t=${TIME_SHOT_T}`);
          await expect(page.getByTestId('fight-stage')).toHaveAttribute('data-state', 'ready', {
            timeout: 15_000,
          });
          await page.waitForTimeout(250);
          await page.screenshot({
            path: `${DIR}/arena-time-${log}-${lang}-${size.name}-${theme}.png`,
            fullPage: true,
          });
          const overflow = await page.evaluate(
            () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
          );
          expect(overflow).toBeLessThanOrEqual(0);
        });
      }
    }

    test(`arena-time-end ${log} ${lang} mobile`, async ({ page }) => {
      await page.setViewportSize(SIZES[0]);
      await page.goto(`/${lang}/dev/arena?log=${log}&t=99999999999`);
      await expect(page.getByTestId('fight-stage')).toHaveAttribute('data-state', 'finished', {
        timeout: 15_000,
      });
      await page.screenshot({
        path: `${DIR}/arena-time-end-${log}-${lang}-mobile.png`,
        fullPage: true,
      });
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
    });
  }
}

// 008 plan-d step 12: login (with and without an error), onboarding and account, at 375 and
// 1280 px, dark and light, en and es. One login per test (its own x-e2e-client, so its own
// AUTH_IP bucket). Checked against RF-D5 and "UI" of the spec: a 28 rem column at most, 44 px
// targets, no horizontal scroll and the alias truncated in the header at 375 px.
for (const lang of ['es', 'en'] as const) {
  for (const theme of THEMES) {
    authTest(`auth screens ${lang} ${theme}`, async ({ page, identity }) => {
      await page.context().addCookies([{ name: 'theme', value: theme, url: PUBLIC_ORIGIN }]);
      const shoot = async (name: string) => {
        for (const size of SIZES) {
          await page.setViewportSize(size);
          await page.screenshot({
            path: `${DIR}/${name}-${lang}-${size.name}-${theme}.png`,
            fullPage: true,
          });
          await expectLayout(page, size.width);
        }
      };
      await page.goto(`/${lang}/login`);
      await shoot('login');
      await page.goto(`/${lang}/login?error=auth.invalid_state`);
      await shoot('login-error');
      await authLogin(page, identity, lang);
      await shoot('onboarding');
      await page.goto(`/${lang}/account`);
      await shoot('account');
      // 009 step 13: the way back after a wrong identity, and both places of the panel.
      await page.goto(`/${lang}/login?error=auth.reauth_mismatch`);
      await shoot('login-reauth-mismatch');
      await seedOauthAccount(identity, 'google');
      await page.goto(`/${lang}/account`);
      await waitForHydration(page);
      await expectPanelButtons(page, 'reauth-panel-link');
      await shoot('account-reauth-link');
      await press(page.getByTestId('unlink-google'));
      await expectPanelButtons(page, 'reauth-panel-google');
      await shoot('account-reauth-row');
    });
  }
}

/** 009 RF22: full width and 44 px at 375 px, `sm:w-auto` on desktop, inside the provider card. */
async function expectPanelButtons(page: Page, testid: string): Promise<void> {
  const panel = page.getByTestId(testid);
  const original = page.viewportSize();
  for (const size of SIZES) {
    await page.setViewportSize(size);
    const area = await panel.boundingBox();
    const button = await panel.getByTestId('reauth-provider-dev').boundingBox();
    expect(button?.height ?? 0, `height at ${size.width}`).toBeGreaterThanOrEqual(44);
    if (size.width === 375) expect(button?.width, 'full width at 375').toBe(area?.width);
    else expect(button?.width ?? 0, 'auto width at 1280').toBeLessThan(area?.width ?? 0);
  }
  if (original) await page.setViewportSize(original);
}

/** No horizontal scroll, the content column within 28 rem and 44 px targets in <main>. */
async function expectLayout(page: Page, width: number): Promise<void> {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow, `overflow at ${width}`).toBeLessThanOrEqual(0);
  const column = await page.locator('main section').first().boundingBox();
  expect(column?.width ?? 0).toBeLessThanOrEqual(448);
  const small = await page.evaluate(() =>
    [...document.querySelectorAll('main button, main a, main input')]
      .filter((el) => (el as HTMLElement).offsetParent !== null)
      .filter((el) => el.getBoundingClientRect().height < 44)
      .map((el) => el.outerHTML.slice(0, 80)),
  );
  expect(small, `targets under 44 px at ${width}`).toEqual([]);
  const header = await page.getByTestId('header-switchers').boundingBox();
  expect((header?.x ?? 0) + (header?.width ?? 0)).toBeLessThanOrEqual(width);
}
