import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';

const ORIGIN = 'http://localhost:4173';

async function seriousViolations(page: Page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
    .analyze();
  return results.violations
    .filter((v) => v.impact === 'serious' || v.impact === 'critical')
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(' | ')}`);
}

test.describe('test 4: axe without serious or critical violations', () => {
  for (const theme of ['light', 'dark'] as const) {
    test(`home and 404 with theme=${theme}`, async ({ page }) => {
      await page.context().addCookies([{ name: 'theme', value: theme, url: ORIGIN }]);
      // `/api/x` renders the 404 on an excluded path (QA H2).
      for (const path of ['/es/', '/en/', '/es/no-existe', '/api/x']) {
        await page.goto(path);
        expect(await seriousViolations(page), path).toEqual([]);
      }
    });

    test(`catalog with ?theme=${theme}`, async ({ page }) => {
      await page.goto(`/es/dev/components?theme=${theme}`);
      expect(await seriousViolations(page)).toEqual([]);
    });
  }
});

test('RF-A17: translated 404 page', async ({ page }) => {
  const res = await page.goto('/es/no-existe');
  expect(res?.status()).toBe(404);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Página no encontrada');
  await page.goto('/en/no-existe');
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Page not found');
  await expect(page.getByTestId('error-id')).toHaveCount(0);
});

test('focus: every element reached with Tab in the catalog shows a ring', async ({ page }) => {
  await page.goto('/es/dev/components?theme=light');
  const seen = new Set<string>();
  const missing: string[] = [];
  for (let i = 0; i < 120; i++) {
    await page.keyboard.press('Tab');
    const info = await page.evaluate(() => {
      const el = document.activeElement as HTMLElement | null;
      if (!el || el === document.body) return null;
      const id = `${el.tagName}|${el.textContent?.trim().slice(0, 30)}|${el.getAttribute('aria-label') ?? ''}|${el.getBoundingClientRect().top}`;
      return { id, shadow: getComputedStyle(el).boxShadow, label: el.outerHTML.slice(0, 120) };
    });
    if (!info) continue;
    if (seen.has(info.id)) break;
    seen.add(info.id);
    if (info.shadow === 'none') missing.push(info.label);
  }
  expect(seen.size).toBeGreaterThan(20);
  expect(missing).toEqual([]);
});

test('targets of 44 px: catalog defaults and header controls', async ({ page }) => {
  await page.setViewportSize({ width: 375, height: 812 });
  await page.goto('/es/dev/components?theme=light');
  const small = await page.evaluate(() => {
    const out: string[] = [];
    const nodes = document.querySelectorAll<HTMLElement>(
      'header a, header button, [data-testid="catalog"] button, [data-testid="catalog"] a, [data-testid="catalog"] input',
    );
    for (const el of nodes) {
      const rect = el.getBoundingClientRect();
      if (rect.width === 0 && rect.height === 0) continue;
      if (rect.width < 44 || rect.height < 44)
        out.push(
          `${Math.round(rect.width)}x${Math.round(rect.height)} ${el.outerHTML.slice(0, 100)}`,
        );
    }
    return out;
  });
  expect(small).toEqual([]);
});

test('test 9: reduced motion zeroes transitions and animations', async ({ browser }) => {
  const context = await browser.newContext({ reducedMotion: 'reduce' });
  const page = await context.newPage();
  await page.goto('/es/dev/components?theme=light');
  const ms = (value: string) =>
    Math.max(
      ...value
        .split(',')
        .map((v) => (v.trim().endsWith('ms') ? parseFloat(v) : parseFloat(v) * 1000)),
    );
  const button = await page.getByTestId('button-default').evaluate((el) => {
    const s = getComputedStyle(el);
    return { transition: s.transitionDuration, animation: s.animationDuration };
  });
  expect(ms(button.transition)).toBeLessThanOrEqual(0.01);
  expect(ms(button.animation)).toBeLessThanOrEqual(0.01);
  await page.getByTestId('dialog-trigger').click();
  const dialog = await page.getByTestId('dialog-content').evaluate((el) => {
    const s = getComputedStyle(el);
    return { transition: s.transitionDuration, animation: s.animationDuration };
  });
  expect(ms(dialog.transition)).toBeLessThanOrEqual(0.01);
  expect(ms(dialog.animation)).toBeLessThanOrEqual(0.01);
  await context.close();
});
