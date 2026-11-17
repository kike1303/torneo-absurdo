import type { Page } from '@playwright/test';
import { seedYesterdayUser } from './support/db.mjs';
import { waitForHydration } from './support/hydration.js';
import { expect, identityFor, login, newContext, press, test, text } from './auth/fixtures.js';

// 024 phase 2 (TA-93): the shop with its preview → buy an animation and an accessory with the coins
// of the bets, the balance going down → equip them in /botarga → they travel in a fight. Then an
// item the balance does not reach, without a buy button. The account is from yesterday, with a
// balance and no botarga yet: /botarga creates it. A rival is created first, so the search always
// has one (the stack has no roster). Captures at 375 px and on desktop, light and dark.

const DIR = 'test-results/screenshots';
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;
const START = 1500;
const BEANIE = 750;
const IDLE = 400;
const MITTS = 750;

const coins = (n: number) => new Intl.NumberFormat('es', { maximumFractionDigits: 0 }).format(n);

/** Logs in on /botarga, past the onboarding, and creates the botarga if it has none. */
async function toBotarga(page: Page, identity: string): Promise<void> {
  await login(page, identity, 'es', '/es/botarga');
  if (page.url().includes('/onboarding')) await press(page.getByTestId('onboarding-skip'));
  await page.waitForURL('/es/botarga');
  await waitForHydration(page);
  if ((await page.getByTestId('botarga-page').getAttribute('data-state')) === 'none') {
    await press(page.getByTestId('botarga-create'));
  }
  await expect(page.getByTestId('botarga-page')).toHaveAttribute('data-state', 'ready');
}

async function openShop(page: Page, item?: string): Promise<void> {
  await page.goto(item === undefined ? '/es/shop' : `/es/shop?item=${item}`);
  await waitForHydration(page);
  await expect(page.getByTestId('shop-page')).toHaveAttribute('data-state', 'ready');
}

async function buy(page: Page, item: string, balanceAfter: number): Promise<void> {
  await press(page.locator(`[data-testid="shop-item"][data-item="${item}"]`));
  await expect(page).toHaveURL(new RegExp(`\\?item=${item}$`));
  const confirm = page.getByTestId('shop-confirm');
  await expect(confirm).toHaveAttribute('data-state', 'buyable');
  await press(confirm.getByTestId('shop-buy'));
  await expect(page.getByTestId('shop-bought')).toBeVisible();
  await expect(page.getByTestId('shop-balance')).toHaveText(
    text('es', 'shop_balance', { amount: coins(balanceAfter) }),
  );
  await expect(page.locator(`[data-testid="shop-item"][data-item="${item}"]`)).toHaveAttribute(
    'data-state',
    'owned',
  );
}

async function noHorizontalScroll(page: Page): Promise<void> {
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  );
  expect(overflow).toBeLessThanOrEqual(0);
}

test('buy an animation and an accessory, equip them and wear them in a fight', async ({
  page,
  browser,
}, info) => {
  test.setTimeout(120_000);
  const rival = await newContext(browser, info, 'rival');
  await toBotarga(await rival.newPage(), identityFor(info, 'rival'));
  await rival.close();

  await seedYesterdayUser(identityFor(info), START);
  await toBotarga(page, identityFor(info));
  await press(page.getByTestId('botarga-shop'));
  await page.waitForURL('/es/shop');
  await waitForHydration(page);
  await expect(page.getByTestId('shop-balance')).toHaveText(
    text('es', 'shop_balance', { amount: coins(START) }),
  );
  await expect(page.getByTestId('shop-pick-hint')).toBeVisible();

  // Picking an item tries it on in 3D and opens its confirmation, with the balance it leaves.
  await press(page.locator('[data-testid="shop-item"][data-item="cosmetic:idle_2"]'));
  await expect(page.getByTestId('botarga-preview')).toHaveAttribute('data-phase', /ready|paused/, {
    timeout: 30_000,
  });
  await expect(page.getByTestId('shop-confirm')).toContainText(
    text('es', 'shop_after', { amount: coins(START - IDLE) }),
  );
  await buy(page, 'cosmetic:idle_2', START - IDLE);
  await buy(page, 'accessory:oven_mitts', START - IDLE - MITTS);

  // What the balance does not reach has no buy button: a tier-1 accessory.
  await openShop(page, 'accessory:propeller_beanie');
  await expect(page.getByTestId('shop-confirm-note')).toHaveText(
    text('es', 'shop_short', { amount: coins(BEANIE - (START - IDLE - MITTS)) }),
  );
  await expect(page.getByTestId('shop-buy')).toHaveCount(0);
  // An item of a tier out of reach says the level that opens it.
  const locked = page.locator('[data-testid="shop-item"][data-state="level"]').first();
  await expect(locked).toContainText(text('es', 'shop_state_level', { level: '20' }));

  // Equipped in /botarga, they are what the botarga wears.
  await page.goto('/es/botarga');
  await waitForHydration(page);
  await page.getByLabel(text('es', 'botarga_idle')).selectOption('idle_2');
  await page.getByTestId('botarga-wear-hands').selectOption('oven_mitts');
  await press(page.getByTestId('botarga-save'));
  await expect(page.getByTestId('botarga-saved')).toHaveText(text('es', 'botarga_saved_build'));
  await expect(page.getByTestId('botarga-preview')).toHaveAttribute('data-phase', /ready|paused/, {
    timeout: 30_000,
  });

  // And they travel in a fight.
  await press(page.getByTestId('pvp-search'));
  const attack = page.getByTestId('pvp-attack');
  await expect(attack).toBeVisible({ timeout: 20_000 });
  const link = await attack
    .getByRole('link', { name: text('es', 'pvp_fight_link') })
    .getAttribute('href');
  const fightId = (link ?? '').split('/').at(-1) ?? '';
  const detail = (await (await page.request.get(`/api/pvp/fights/${fightId}`)).json()) as {
    look_a: { idle: string | null; accessories: string[]; skin: string | null };
  };
  expect(detail.look_a).toMatchObject({ idle: 'idle_2', accessories: ['oven_mitts'], skin: null });

  // Captures for review, and no horizontal scroll.
  for (const size of SIZES) {
    for (const theme of ['light', 'dark'] as const) {
      await page
        .context()
        .addCookies([{ name: 'theme', value: theme, url: 'http://localhost:4173' }]);
      await page.setViewportSize(size);
      for (const [name, item] of [
        ['shop', undefined],
        ['shop-confirm', 'accessory:tinfoil_crown'],
        // A suit (TA-94), tried on in the 3D preview.
        ['shop-skin', 'skin:astronaut'],
      ] as const) {
        await openShop(page, item);
        if (item !== undefined) {
          await expect(page.getByTestId('botarga-preview')).toHaveAttribute(
            'data-phase',
            /ready|paused/,
            { timeout: 30_000 },
          );
        }
        await noHorizontalScroll(page);
        await page.screenshot({ path: `${DIR}/${name}-${size.name}-${theme}.png`, fullPage: true });
      }
    }
  }
});
