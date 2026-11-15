import type { Page } from '@playwright/test';
import { addDynamicCanary } from './support/logs.js';
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

// 023 "Web" (TA-88): two users become friends by the invite link, one challenges the other and
// both watch the private replay, verified in the browser with the engine; a third user gets a 404.
// The invite code is a canary: it lives in the fragment and the tab's `sessionStorage`, so no log
// may carry it. Captures at 375 px and on desktop, light and dark, for review.

const DIR = 'test-results/screenshots';
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;

/** Logs in (with `next`) and skips the onboarding if it shows: the botarga is born with the user. */
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

test('friends: an invite by link, a challenge and its private replay', async ({
  page,
  browser,
}, info) => {
  test.setTimeout(150_000);

  // The host: its link, built in the browser with the code in the fragment.
  await enter(page, identityFor(info), 'es', '/es/friends');
  await expect(page.getByTestId('friends-page')).toHaveAttribute('data-state', 'ready');
  await expect(page.getByTestId('friends-empty')).toBeVisible();
  const linkInput = page.getByTestId('friends-invite-link');
  await expect(linkInput).toHaveValue(/\/es\/friends\/invite#[A-Za-z0-9_-]{22}$/);
  await expect(page.getByTestId('friends-invite-copy')).toBeEnabled();
  const link = await linkInput.inputValue();
  const code = link.split('#')[1] ?? '';
  addDynamicCanary(code);
  const hostName = String((await me(page)).display_name);

  // The guest opens the link without a session: only that there is an invite, no name.
  const guest = await newContext(browser, info, 'guest');
  const guestPage = await guest.newPage();
  await guestPage.goto(new URL(link).pathname + new URL(link).hash);
  await waitForHydration(guestPage);
  const invitePage = guestPage.getByTestId('friend-invite-page');
  await expect(invitePage).toHaveAttribute('data-state', 'anonymous');
  await expect(invitePage).not.toContainText(hostName);
  await expect(guestPage.getByTestId('friend-invite-login')).toHaveAttribute(
    'href',
    `/es/login?next=${encodeURIComponent('/es/friends/invite')}`,
  );

  // After the login, the same tab still has the code: whose invite it is, and an explicit accept.
  await enter(guestPage, identityFor(info, 'guest'), 'es', '/es/friends/invite');
  await expect(guestPage.getByTestId('friend-invite-owner')).toHaveText(
    text('es', 'friends_invite_from', { name: hostName }),
  );
  const guestName = String((await me(guestPage)).display_name);
  await press(guestPage.getByTestId('friend-invite-accept'));
  await expect(guestPage.getByTestId('friend-invite-accepted')).toHaveText(
    text('es', 'friends_invite_accepted', { name: hostName }),
  );

  // The host sees the guest and challenges them: the replay is verified and the result waits.
  await page.reload();
  await waitForHydration(page);
  const row = page.getByTestId('friend-row').filter({ hasText: guestName });
  await expect(row).toHaveAttribute('data-ready', 'true');
  await press(row.getByTestId('friend-challenge'));
  const fight = page.getByTestId('friend-challenge-fight');
  await expect(fight).toBeVisible({ timeout: 20_000 });
  // A second challenge before the result is seen is a new fight, never the first one again.
  const first = await page.getByTestId('friend-fight-link').getAttribute('href');
  await press(row.getByTestId('friend-challenge'));
  await expect(page.getByTestId('friend-fight-link')).not.toHaveAttribute('href', first ?? '', {
    timeout: 20_000,
  });
  await expect(page.getByTestId('pvp-result')).toHaveCount(0);
  await expect(page.getByTestId('pvp-verdict')).toHaveAttribute('data-status', 'match', {
    timeout: 60_000,
  });
  await press(page.getByTestId('pvp-show-result'));
  await expect(page.getByTestId('pvp-result')).toBeVisible();
  const fightLink = await page.getByTestId('friend-fight-link').getAttribute('href');
  expect(fightLink).toMatch(/^\/es\/friends\/fights\/[1-9][0-9]*$/);
  await expect(page.getByTestId('friend-fight-row').first()).toHaveAttribute(
    'data-role',
    'attacker',
  );

  // The guest watches it too, as the challenged side, verified.
  await guestPage.goto('/es/friends');
  await waitForHydration(guestPage);
  await expect(guestPage.getByTestId('friend-fight-row').first()).toHaveAttribute(
    'data-role',
    'defender',
  );
  await guestPage.goto(fightLink ?? '/');
  await waitForHydration(guestPage);
  await expect(guestPage.getByTestId('friend-fight-page')).toHaveAttribute('data-status', 'ready');
  await expect(guestPage.getByTestId('pvp-verdict')).toHaveAttribute('data-status', 'match', {
    timeout: 60_000,
  });

  // Changing the link: the old one stops working, the friendship stays.
  await guestPage.goto('/es/friends');
  await waitForHydration(guestPage);
  await press(guestPage.getByTestId('friends-invite-rotate'));
  await expect(guestPage.getByTestId('friends-saved')).toHaveText(
    text('es', 'friends_saved_rotated'),
  );
  await expect(guestPage.getByTestId('friend-row')).toHaveCount(1);

  // Private: a third user gets a 404, and without a session it goes to the login.
  const stranger = await newContext(browser, info, 'stranger');
  const strangerPage = await stranger.newPage();
  await enter(strangerPage, identityFor(info, 'stranger'), 'es', '/es/friends');
  const response = await strangerPage.goto(fightLink ?? '/');
  expect(response?.status()).toBe(404);
  await stranger.close();
  const visitor = await newContext(browser, info, 'visitor');
  const anonymous = await visitor.newPage();
  await anonymous.goto(fightLink ?? '/');
  await expect(anonymous).toHaveURL(/\/es\/login\?next=/);
  await visitor.close();

  // Captures for review, and no horizontal scroll.
  for (const size of SIZES) {
    for (const theme of ['light', 'dark'] as const) {
      for (const p of [page, guestPage]) {
        await p
          .context()
          .addCookies([{ name: 'theme', value: theme, url: 'http://localhost:4173' }]);
        await p.setViewportSize(size);
      }
      for (const [name, p, path] of [
        ['friends', page, '/es/friends'],
        ['friends-replay', page, fightLink ?? '/'],
        ['friends-en', guestPage, '/en/friends'],
      ] as const) {
        await p.goto(path);
        await waitForHydration(p);
        if (name === 'friends-replay') {
          await expect(p.getByTestId('pvp-verdict')).toHaveAttribute('data-status', 'match', {
            timeout: 60_000,
          });
        }
        await noHorizontalScroll(p);
        await p.screenshot({ path: `${DIR}/${name}-${size.name}-${theme}.png`, fullPage: true });
      }
    }
  }

  // Removing: after the confirmation, both lists are empty.
  await page.goto('/es/friends');
  await waitForHydration(page);
  await press(page.getByTestId('friend-remove'));
  await press(page.getByTestId('friend-remove-yes'));
  await expect(page.getByTestId('friends-saved')).toHaveText(text('es', 'friends_saved_removed'));
  await expect(page.getByTestId('friends-empty')).toBeVisible();
  await guestPage.goto('/es/friends');
  await waitForHydration(guestPage);
  await expect(guestPage.getByTestId('friends-empty')).toBeVisible();
  await guest.close();
});
