import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';
import spike from '@torneo/renderer/fixtures/spike.timeline.json' with { type: 'json' };
import { waitForHydration } from './support/hydration.js';

// 002 RF-A15, RF-A16 and RF-A19 (plan-a2 step 7): the test arena on the production build, with
// WebGL by software in Chromium. Screenshots are artifacts for review, not pixel comparisons.
const ORIGIN = 'http://localhost:4173';
const DIR = 'test-results/screenshots';
const DURATION = spike.duration_ms;
const stage = (page: Page) => page.getByTestId('fight-stage');

function trackErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('console', (message) => {
    if (message.type() === 'error') errors.push(message.text());
  });
  page.on('pageerror', (error) => errors.push(error.message));
  return errors;
}

async function seriousViolations(page: Page) {
  const results = await new AxeBuilder({ page })
    .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
    .analyze();
  return results.violations
    .filter((v) => v.impact === 'serious' || v.impact === 'critical')
    .map((v) => `${v.id}: ${v.nodes.map((n) => n.target.join(' ')).join(' | ')}`);
}

for (const lang of ['es', 'en'] as const) {
  test(`/${lang}/dev/arena reaches playing without console errors`, async ({ page }) => {
    const errors = trackErrors(page);
    await page.goto(`/${lang}/dev/arena`);
    await expect(stage(page)).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
    await expect(
      page.getByRole('img', { name: lang === 'es' ? /Muñeco A/ : /Dummy A/ }),
    ).toBeVisible();
    await page.waitForTimeout(1000);
    expect(errors).toEqual([]);
  });
}

test('?t=<duration_ms> reaches finished and announces the winner', async ({ page }) => {
  await page.goto(`/es/dev/arena?t=${DURATION}`);
  await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
  await expect(page.getByTestId('fight-winner')).toHaveText('Gana Muñeco A');
  await waitForHydration(page);
  await page.getByRole('button', { name: 'Repetir' }).click();
  await expect(stage(page)).toHaveAttribute('data-state', 'playing');
});

test('HP changes after a strike and the controls pause and resume', async ({ page }) => {
  const strike = spike.cues.find((c) => c.type === 'strike' && (c.damage ?? 0) > 0);
  if (strike === undefined) throw new Error('no strike with damage');
  await page.goto(`/es/dev/arena?t=${strike.at_ms - 400}`);
  await expect(stage(page)).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
  const before = [
    await stage(page).getAttribute('data-hp-a'),
    await stage(page).getAttribute('data-hp-b'),
  ];
  await expect
    .poll(async () => [
      await stage(page).getAttribute('data-hp-a'),
      await stage(page).getAttribute('data-hp-b'),
    ])
    .not.toEqual(before);
  await waitForHydration(page);
  await page.getByTestId('arena-toggle').click();
  await expect(stage(page)).toHaveAttribute('data-state', 'paused');
  await page.getByTestId('arena-toggle').click();
  await expect(stage(page)).toHaveAttribute('data-state', 'playing');
});

test('RF-A19: after a seek to the middle and 3 s of playback, frames > 0', async ({ page }) => {
  await page.goto(`/es/dev/arena?t=${Math.round(DURATION / 2)}`);
  await expect(stage(page)).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
  await waitForHydration(page);
  await page.getByTestId('arena-show-stats').check();
  await page.waitForTimeout(3000);
  await expect
    .poll(async () =>
      Number((await page.getByTestId('arena-stats-frames').textContent())?.replace(/\D/g, '')),
    )
    .toBeGreaterThan(0);
});

test('changing a fighter restarts the arena', async ({ page }) => {
  await page.goto('/en/dev/arena');
  await expect(stage(page)).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
  await waitForHydration(page);
  await page.getByTestId('arena-fighter-b').selectOption('dummy_a');
  await expect(
    page.getByRole('img', { name: 'Animated fight between Dummy A and Dummy A' }),
  ).toBeAttached();
  await expect(stage(page)).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
});

test('the instance without the catalog answers 404', async ({ request }) => {
  const res = await request.get('http://localhost:4174/es/dev/arena');
  expect(res.status()).toBe(404);
});

for (const theme of ['light', 'dark'] as const) {
  test(`axe without serious or critical violations (theme=${theme})`, async ({ page }) => {
    await page.context().addCookies([{ name: 'theme', value: theme, url: ORIGIN }]);
    await page.goto('/es/dev/arena');
    await expect(stage(page)).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
    await waitForHydration(page);
    await page.getByTestId('arena-show-stats').check();
    // The controls fade in from `disabled:opacity-50` (transition-all); axe must see them settled.
    await expect(page.getByTestId('arena-toggle')).toBeEnabled();
    await page.waitForTimeout(500);
    expect(await seriousViolations(page)).toEqual([]);
  });
}

const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;
for (const size of SIZES) {
  for (const theme of ['light', 'dark'] as const) {
    for (const lang of ['es', 'en'] as const) {
      test(`screenshot ${lang} ${size.name} ${theme}, without horizontal scroll`, async ({
        page,
      }) => {
        await page.context().addCookies([{ name: 'theme', value: theme, url: ORIGIN }]);
        await page.setViewportSize(size);
        // Just after a crit: numbers, particles, flash and trail are on screen.
        await page.goto(`/${lang}/dev/arena?t=12500`);
        await expect(stage(page)).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
        await page.waitForTimeout(250);
        const box = await stage(page).boundingBox();
        expect(box?.width ?? Infinity).toBeLessThanOrEqual(size.width);
        const overflow = await page.evaluate(
          () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
        );
        expect(overflow).toBeLessThanOrEqual(0);
        await page.screenshot({
          path: `${DIR}/arena-${lang}-${size.name}-${theme}.png`,
          fullPage: true,
        });
      });
    }
  }
}
