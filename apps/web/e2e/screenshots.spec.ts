import { expect, test } from '@playwright/test';

// Test 8 and the 375 px RNF: artifacts for review (no pixel comparison), plus no horizontal scroll.
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;
const THEMES = ['light', 'dark'] as const;
const DIR = 'test-results/screenshots';

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
