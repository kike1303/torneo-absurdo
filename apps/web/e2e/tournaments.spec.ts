import type { Page } from '@playwright/test';
import { waitForHydration } from './support/hydration.js';
import {
  expect,
  identityFor,
  login,
  me,
  newContext,
  press,
  test,
  text,
  type Locale,
} from './auth/fixtures.js';

// 023 "Web" (TA-89): a host and three friends; the host sets up a tournament of 4 and lands on its
// bracket. The results wait for "show results": until then only round 1 has names, each match
// with its private replay, verified in the browser. A player reads it too; a stranger gets a 404.
// The friendships are made by the API (the invite flow is `friends.spec.ts`). Captures at 375 px
// and on desktop, light and dark, for review.

const DIR = 'test-results/screenshots';
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;

async function enter(page: Page, identity: string, locale: Locale, next: string): Promise<void> {
  await login(page, identity, locale, next);
  if (page.url().includes('/onboarding')) {
    await press(page.getByTestId('onboarding-skip'));
    if (!page.url().endsWith(next)) await page.goto(next);
  }
  await page.waitForURL((url) => url.pathname === next);
  await waitForHydration(page);
}

async function noHorizontalScroll(page: Page): Promise<void> {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
}

test('tournaments: a bracket of 4, its hidden results and its private matches', async ({
  page,
  browser,
}, info) => {
  test.setTimeout(150_000);
  await enter(page, identityFor(info), 'es', '/es/friends');
  const invite = await page.request.get('/api/friends');
  const code = ((await invite.json()) as { invite: string }).invite;

  // Three friends, each accepting the host's code.
  const guests = [];
  for (const tag of ['b', 'c', 'd']) {
    const context = await newContext(browser, info, tag);
    const guest = await context.newPage();
    await enter(guest, identityFor(info, tag), 'es', '/es/friends');
    const origin = new URL(guest.url()).origin;
    const accepted = await guest.request.post('/api/friends/accept', {
      data: { code },
      headers: { origin },
    });
    expect(accepted.status()).toBe(200);
    guests.push({ context, page: guest });
  }

  // The host picks the three and is taken to the bracket.
  await page.reload();
  await waitForHydration(page);
  const form = page.getByTestId('tournament-new');
  for (const pick of await form.getByTestId('tournament-pick').all()) await pick.check();
  await press(page.getByTestId('tournament-create'));
  await page.waitForURL(/\/es\/friends\/tournaments\/[1-9][0-9]*$/, { timeout: 30_000 });
  await waitForHydration(page);
  const bracketUrl = new URL(page.url()).pathname;
  await expect(page.getByTestId('tournament-page')).toHaveAttribute('data-status', 'ready');

  // No outcome travels in the HTML, of the bracket nor of my friends page (011 RF18, RF20).
  for (const path of [bracketUrl, '/es/friends']) {
    const html = await (await page.request.get(path)).text();
    expect(html).not.toContain('champion');
    expect(html).not.toContain('advances');
  }

  // Hidden: round 1 has its two matches with names and links; the final has neither.
  await expect(page.getByTestId('tournament-champion')).toHaveCount(0);
  await expect(page.getByTestId('bracket-advances')).toHaveCount(0);
  const rounds = page.getByTestId('bracket-round');
  await expect(rounds).toHaveCount(2);
  await expect(rounds.nth(0).getByTestId('bracket-match')).toHaveCount(2);
  await expect(rounds.nth(0).getByTestId('bracket-watch')).toHaveCount(2);
  await expect(rounds.nth(1).getByTestId('bracket-match')).toHaveAttribute('data-shown', 'false');
  await expect(rounds.nth(1).getByTestId('bracket-watch')).toHaveCount(0);

  // A match of round 1 replays, verified.
  const matchLink = await rounds.nth(0).getByTestId('bracket-watch').first().getAttribute('href');
  expect(matchLink).toMatch(/^\/es\/friends\/fights\/[1-9][0-9]*$/);
  await page.goto(matchLink ?? '/');
  await waitForHydration(page);
  await expect(page.getByTestId('pvp-verdict')).toHaveAttribute('data-status', 'match', {
    timeout: 60_000,
  });

  // "Show results": the champion, who advanced in each match, and the final's link.
  await page.goto(bracketUrl);
  await waitForHydration(page);
  await press(page.getByTestId('tournament-show-results'));
  const players = [String((await me(page)).display_name)];
  for (const guest of guests) players.push(String((await me(guest.page)).display_name));
  const champion = page.getByTestId('tournament-champion');
  await expect(champion).toBeVisible();
  const championText = (await champion.textContent()) ?? '';
  expect(
    players.some((name) => championText.includes(text('es', 'tournament_champion', { name }))),
  ).toBe(true);
  await expect(page.getByTestId('bracket-advances')).toHaveCount(3);
  await expect(rounds.nth(1).getByTestId('bracket-watch')).toHaveCount(1);

  // A player reads it and finds it in their list; a stranger gets a 404.
  const player = guests[0]!.page;
  await player.goto('/es/friends');
  await waitForHydration(player);
  await expect(player.getByTestId('tournament-row')).toHaveCount(1);
  await expect(player.getByTestId('tournament-watch')).toHaveAttribute('href', bracketUrl);
  // The list of fights has no match of a tournament: how many one played would say how far one
  // went.
  await expect(player.getByTestId('friend-fight-row')).toHaveCount(0);
  const read = await player.goto(bracketUrl);
  expect(read?.status()).toBe(200);
  const strangerContext = await newContext(browser, info, 'stranger');
  const stranger = await strangerContext.newPage();
  await enter(stranger, identityFor(info, 'stranger'), 'es', '/es/friends');
  expect((await stranger.goto(bracketUrl))?.status()).toBe(404);
  expect((await stranger.goto(matchLink ?? '/'))?.status()).toBe(404);
  await strangerContext.close();

  // Captures for review, and no horizontal scroll.
  for (const size of SIZES) {
    for (const theme of ['light', 'dark'] as const) {
      await page
        .context()
        .addCookies([{ name: 'theme', value: theme, url: 'http://localhost:4173' }]);
      await page.setViewportSize(size);
      for (const [name, path, reveal] of [
        ['tournament-hidden', bracketUrl, false],
        ['tournament-results', bracketUrl, true],
        ['tournament-friends', '/es/friends', false],
      ] as const) {
        await page.goto(path);
        await waitForHydration(page);
        if (reveal) await press(page.getByTestId('tournament-show-results'));
        await noHorizontalScroll(page);
        await page.screenshot({ path: `${DIR}/${name}-${size.name}-${theme}.png`, fullPage: true });
      }
    }
  }
  for (const guest of guests) await guest.context.close();
});
