import type { Page } from '@playwright/test';
import { seedAccessory } from './support/db.mjs';
import { waitForHydration } from './support/hydration.js';
import {
  expect,
  identityFor,
  login,
  newContext,
  press,
  test,
  text,
  type Locale,
} from './auth/fixtures.js';

// 021 "Web" (TA-65): onboarding → my botarga → find a fight → the replay, verified in the browser
// with the engine; then my fights, the public replay without a session and the PvP leaderboard.
// The stack has no roster (`CONTENT_DIR` empty), so a second user is created first: the search
// always has a rival. Captures at 375 px and on desktop, light and dark, for review.

const DIR = 'test-results/screenshots';
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;

/** Logs in, skips the onboarding (its botarga is born with the user) and lands on /botarga. */
async function toBotarga(page: Page, identity: string, locale: Locale): Promise<void> {
  await login(page, identity, locale, `/${locale}/botarga`);
  if (page.url().includes('/onboarding')) await press(page.getByTestId('onboarding-skip'));
  await page.waitForURL(`/${locale}/botarga`);
  await waitForHydration(page);
}

async function noHorizontalScroll(page: Page): Promise<void> {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
}

test('find a fight: the replay is verified, and it shows in my fights and in public', async ({
  page,
  browser,
}, info) => {
  test.setTimeout(120_000);
  // The rival: another user with the botarga of a new user.
  const rival = await newContext(browser, info, 'rival');
  await toBotarga(await rival.newPage(), identityFor(info, 'rival'), 'es');
  await rival.close();

  // An accessory won at level 10 (TA-67), so the wardrobe has something to put on.
  await toBotarga(page, identityFor(info), 'es');
  await seedAccessory(identityFor(info), 'oven_mitts');
  await page.reload();
  await waitForHydration(page);
  const botarga = page.getByTestId('botarga-page');
  await expect(botarga).toHaveAttribute('data-state', 'ready');
  await expect(page.getByTestId('botarga-level')).toHaveText(
    text('es', 'botarga_level', { level: '1' }),
  );
  // Its botarga in 3D, paused in its idle until "see its victory".
  await expect(page.getByTestId('botarga-preview')).toHaveAttribute('data-phase', /ready|paused/, {
    timeout: 30_000,
  });
  await expect(page.getByTestId('botarga-ability')).toHaveCount(1);

  // A build saved by its form action: the server keeps the style and the accessory picked, and the
  // accessory moves its points (oven mitts: +8 attack, −8 speed).
  await page.getByLabel(text('es', 'botarga_style')).selectOption('aggressive');
  await page.getByTestId('botarga-wear-hands').selectOption('oven_mitts');
  await press(page.getByTestId('botarga-save'));
  await expect(page.getByTestId('botarga-saved')).toHaveText(text('es', 'botarga_saved_build'));
  await expect(page.getByLabel(text('es', 'botarga_style'))).toHaveValue('aggressive');
  await expect(page.getByTestId('botarga-wear-hands')).toHaveValue('oven_mitts');

  await press(page.getByTestId('pvp-search'));
  const attack = page.getByTestId('pvp-attack');
  await expect(attack).toBeVisible({ timeout: 20_000 });
  // The result waits for the end or the button (011 RF20), and so do the reward and the progress
  // it changed (owner, 2026-11-14); the verdict is the engine's.
  await expect(page.getByTestId('pvp-result')).toHaveCount(0);
  await expect(page.getByTestId('pvp-reward')).toHaveCount(0);
  await expect(page.getByTestId('botarga-progress')).toContainText(
    text('es', 'botarga_daily', { n: '0' }),
  );
  await expect(page.getByTestId('pvp-verdict')).toHaveAttribute('data-status', 'match', {
    timeout: 60_000,
  });
  await expect(page.getByTestId('pvp-verdict-text')).toHaveText(text('es', 'pvp_verify_match'));
  await press(page.getByTestId('pvp-show-result'));
  await expect(page.getByTestId('pvp-result')).toBeVisible();
  // A new account is in probation: XP, and neither rating nor coins (021 "Carencia PvP").
  await expect(page.getByTestId('pvp-reward-rating')).toHaveText('0');
  await expect(page.getByTestId('pvp-reward-coins')).toHaveText('0');
  await expect(page.getByTestId('pvp-reward-xp')).not.toHaveText('0');
  const link = await attack
    .getByRole('link', { name: text('es', 'pvp_fight_link') })
    .getAttribute('href');
  expect(link).toMatch(/^\/es\/pvp\/fights\/[1-9][0-9]*$/);
  // What it wore travels with the fight, for the rival and the replays (TA-67, TA-84).
  const fightId = (link ?? '').split('/').at(-1) ?? '';
  const detail = (await (await page.request.get(`/api/pvp/fights/${fightId}`)).json()) as {
    look_a: { accessories: string[] };
  };
  expect(detail.look_a.accessories).toEqual(['oven_mitts']);
  await expect(page.getByTestId('botarga-progress')).toContainText(
    text('es', 'botarga_daily', { n: '1' }),
  );

  // My fights, as the attacker.
  await page.goto('/es/pvp/fights');
  await waitForHydration(page);
  const row = page.getByTestId('pvp-fight-row').first();
  await expect(row).toHaveAttribute('data-role', 'attacker');
  await expect(row.getByTestId('pvp-fight-watch')).toHaveAttribute('href', link ?? '');

  // The replay is public: another context, without a session, verifies it too.
  const visitor = await newContext(browser, info, 'visitor');
  const anonymous = await visitor.newPage();
  await anonymous.goto(link ?? '/');
  await waitForHydration(anonymous);
  await expect(anonymous.getByTestId('pvp-fight-page')).toHaveAttribute('data-status', 'ready');
  await expect(anonymous.getByTestId('pvp-verdict')).toHaveAttribute('data-status', 'match', {
    timeout: 60_000,
  });
  await visitor.close();

  // Without a session, my botarga and my fights go to the login.
  const stranger = await newContext(browser, info, 'stranger');
  const outside = await stranger.newPage();
  await outside.goto('/en/botarga');
  await expect(outside).toHaveURL(/\/en\/login\?next=/);
  await stranger.close();

  // The leaderboard is public, in both languages; a new account does not appear in it.
  for (const locale of ['es', 'en'] as const) {
    await page.goto(`/${locale}/pvp/leaderboard`);
    await expect(page.getByTestId('pvp-leaderboard')).toContainText(
      text(locale, 'pvp_leaderboard_title'),
    );
  }

  // Captures for review, and no horizontal scroll.
  for (const size of SIZES) {
    for (const theme of ['light', 'dark'] as const) {
      await page
        .context()
        .addCookies([{ name: 'theme', value: theme, url: 'http://localhost:4173' }]);
      await page.setViewportSize(size);
      for (const [name, path] of [
        ['botarga', '/es/botarga'],
        ['pvp-fights', '/es/pvp/fights'],
        ['pvp-replay', link ?? '/'],
        ['pvp-leaderboard', '/en/pvp/leaderboard'],
      ] as const) {
        await page.goto(path);
        await waitForHydration(page);
        await noHorizontalScroll(page);
        await page.screenshot({ path: `${DIR}/${name}-${size.name}-${theme}.png`, fullPage: true });
      }
    }
  }
});
