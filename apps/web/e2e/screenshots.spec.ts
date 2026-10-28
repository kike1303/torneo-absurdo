import { expect, test } from '@playwright/test';

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
