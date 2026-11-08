import { expect, test, type APIRequestContext, type Page, type TestInfo } from '@playwright/test';
import type { FightsCurrent, MyFightBets } from '@torneo/protocol';
import {
  clientFor,
  identityFor,
  login,
  newContext,
  press,
  text,
  type Locale,
} from './auth/fixtures.js';
import { seedYesterdayUser } from './support/db.mjs';
import { waitForHydration } from './support/hydration.js';
import { EGRESS_PROXY, PUBLIC_ORIGIN } from './support/topology.js';

// 012 RF41 to RF48 on real fights (plan-c C8). The four projects bet on the same three fights of the
// script, each with its own accounts, so no amount of a pool is asserted, only what each account
// sees: a KO with bets on both sides (won, lost, the refusals, the rescue, without JS and a reload
// in the middle of the fight), a draw, and a KO where nobody bets on the second side.
const RUN_LIMIT_MS = 12 * 60_000;
const SHOTS = 'test-results/screenshots';
/** What each project needs of a window to place its bets. */
const WINDOW_NEEDED_MS = 32_000;
/** `DAILY_GRANT` of the e2e: the bonus, and the cap of an account in probation. */
const GRANT = 100;
const SIDES = { ko: ['striker', 'trickster'], draw: ['turtle', 'turtle'] } as const;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, Math.max(0, ms)));
const localeOf = (info: TestInfo): Locale =>
  info.project.name.startsWith('chromium') ? 'es' : 'en';
const panel = (page: Page) => page.getByTestId('bet-panel');

async function boardOf(request: APIRequestContext): Promise<FightsCurrent> {
  const response = await request.get('/api/fights/current');
  expect(response.status()).toBe(200);
  return (await response.json()) as FightsCurrent;
}

/**
 * The first KO of the script (the fight ids of a fresh database are its ranks, so `id % 3 === 1` is
 * the first row) whose window still has room for the bets. The draw and the second KO follow it.
 */
async function firstFight(request: APIRequestContext): Promise<number> {
  for (;;) {
    const board = await boardOf(request);
    const now = Number(board.server_time);
    for (const fight of [board.current, board.next]) {
      if (fight === null || fight.id % 3 !== 1) continue;
      const opensIn = Number(fight.opens_at) - now;
      const closesIn = Number(fight.closes_at) - now;
      if (
        fight.state === 'scheduled' ||
        (fight.state === 'betting_open' && closesIn > WINDOW_NEEDED_MS)
      ) {
        if (opensIn < 140_000) return fight.id;
      }
    }
    await sleep(1000);
  }
}

async function untilOpen(request: APIRequestContext, id: number): Promise<void> {
  await expect
    .poll(
      async () =>
        (await boardOf(request)).current?.state === 'betting_open' &&
        (await boardOf(request)).current?.id === id,
      { timeout: 160_000, intervals: [500] },
    )
    .toBe(true);
}

async function untilLive(request: APIRequestContext, id: number): Promise<void> {
  await expect
    .poll(
      async () => {
        const { current } = await boardOf(request);
        return current?.id === id && current.state === 'live';
      },
      { timeout: 60_000, intervals: [500] },
    )
    .toBe(true);
}

/** The display name the page gives a fighter without lore (`fighter-names.ts`). */
const fighter = (locale: Locale, slug: string) => text(locale, 'fight_name_unknown', { slug });
const coins = (locale: Locale, amount: number) =>
  text(locale, 'coins_amount', { amount: new Intl.NumberFormat(locale).format(amount) });

async function home(page: Page, locale: Locale): Promise<void> {
  await page.goto(`/${locale}/`);
  await waitForHydration(page);
}

/** Side and amount, and the bet. Returns once the server answered. */
async function bet(page: Page, side: 'a' | 'b', amount: number): Promise<void> {
  const form = page.getByTestId('bet-form');
  await expect(form).toBeVisible({ timeout: 20_000 });
  await press(form.getByTestId(`bet-side-${side}`));
  await form.getByTestId('bet-amount').fill(String(amount));
  const answered = page.waitForResponse(
    (r) => r.url().includes('?/bet') && r.request().method() === 'POST',
  );
  await press(form.getByTestId('bet-submit'));
  await answered;
}

async function myBets(request: APIRequestContext, id: number): Promise<MyFightBets> {
  const response = await request.get(`/api/bets/mine?fight_id=${id}`);
  expect(response.status()).toBe(200);
  return (await response.json()) as MyFightBets;
}

async function shot(page: Page, info: TestInfo, name: string): Promise<void> {
  for (const scheme of ['light', 'dark'] as const) {
    await page.emulateMedia({ colorScheme: scheme });
    await panel(page).screenshot({
      path: `${SHOTS}/bets-${info.project.name}-${name}-${scheme}.png`,
    });
  }
  await page.emulateMedia({ colorScheme: null });
}

test('bets on the same real fights in every project', async ({ browser, request }, info) => {
  test.setTimeout(RUN_LIMIT_MS);
  const locale = localeOf(info);
  const t = (key: string, params: Record<string, string> = {}) => text(locale, key, params);
  const contexts: Awaited<ReturnType<typeof newContext>>[] = [];
  const account = async (tag: string): Promise<Page> => {
    const context = await newContext(browser, info, tag);
    contexts.push(context);
    const page = await context.newPage();
    await login(page, identityFor(info, tag), locale);
    await home(page, locale);
    return page;
  };

  try {
    // The bonus with a balance, and without waiting for a fight (RF42): an account from yesterday.
    const danaId = identityFor(info, 'dana');
    await seedYesterdayUser(danaId, GRANT);
    const dana = await account('dana');
    await expect(dana.getByTestId('header-balance')).toHaveText(coins(locale, GRANT));
    const bonus = dana.getByTestId('bet-bonus-claim');
    await expect(bonus).toHaveText(t('bet_bonus_claim', { amount: coins(locale, GRANT) }));
    await press(bonus);
    await expect(bonus).toHaveCount(0);
    await expect(dana.getByTestId('header-balance')).toHaveText(coins(locale, 2 * GRANT));
    await dana.context().close();

    const [alice, bob, eve] = await Promise.all([account('alice'), account('bob'), account('eve')]);
    // Without JS (RF44): the session of a context with JS, in one without it.
    const carolContext = await newContext(browser, info, 'carol');
    contexts.push(carolContext);
    await login(await carolContext.newPage(), identityFor(info, 'carol'), locale);
    const noJs = await browser.newContext({
      baseURL: PUBLIC_ORIGIN,
      proxy: EGRESS_PROXY,
      viewport: info.project.use.viewport,
      userAgent: info.project.use.userAgent,
      javaScriptEnabled: false,
      storageState: await carolContext.storageState(),
      extraHTTPHeaders: { 'x-e2e-client': clientFor(info, 'carol') },
    });
    contexts.push(noJs);
    const carol = await noJs.newPage();

    // The first fight: a KO that `b` wins.
    const ko = await firstFight(request);
    const nameA = fighter(locale, SIDES.ko[0]);
    const nameB = fighter(locale, SIDES.ko[1]);
    await untilOpen(request, ko);

    // A second tab of alice, opened before her first bet: the side it picks is refused.
    const second = await alice.context().newPage();
    await home(second, locale);

    const aliceFlow = async () => {
      // The window opened after the page loaded: the panel reads the bets of this fight by itself.
      const form = alice.getByTestId('bet-form');
      await expect(form).toBeVisible({ timeout: 20_000 });
      const probation = panel(alice).getByTestId('bet-probation');
      await expect(probation).toContainText(t('bet_probation', { cap: coins(locale, GRANT) }));
      // RF48: the explanation, without leaving the panel.
      await press(panel(alice).getByTestId('bet-explainer').locator('summary'));
      await expect(panel(alice).getByText(t('bet_explainer_split'))).toBeVisible();
      await expect(panel(alice).getByText(t('bet_explainer_fictional'))).toBeVisible();
      // RF47: the estimate for that amount and side, with what is lost.
      await press(form.getByTestId('bet-side-a'));
      await form.getByTestId('bet-amount').fill('25');
      const quote = form.getByTestId('bet-quote');
      await expect(quote).toHaveAttribute('data-status', /^(ready|stale)$/, { timeout: 10_000 });
      await expect(quote).toContainText(
        t('bet_quote_lose', { name: nameA, amount: coins(locale, 25) }),
      );
      await expect(form.getByTestId('bet-final-notice')).toHaveText(t('bet_final_notice'));
      await bet(alice, 'a', 25);
      await expect(panel(alice).getByTestId('bet-mine')).toHaveText(
        t('bet_mine', { amount: coins(locale, 25), name: nameA }),
      );
      await shot(alice, info, 'placed');

      // RF26: the other tab picks `b`, and is refused with its amount kept.
      await bet(second, 'b', 30);
      const refused = second.getByTestId('bet-rejected');
      await expect(refused).toHaveAttribute('data-code', 'bet.side_locked');
      await expect(refused).toContainText(t('bet_rejected_side', { name: nameA }));
      await expect(second.getByTestId('bet-amount')).toHaveValue('30');
      await expect(second.getByTestId('bet-side-b').locator('input')).toBeDisabled();
      await second.close();

      // Over the cap of probation: refused with what is left, and the amount kept.
      await bet(alice, 'a', 150);
      const capped = alice.getByTestId('bet-rejected');
      await expect(capped).toHaveAttribute('data-code', 'bet.fight_cap_exceeded');
      await expect(capped).toContainText(
        t('bet_rejected_cap', { cap: coins(locale, GRANT), max: coins(locale, GRANT - 25) }),
      );
      await expect(alice.getByTestId('bet-amount')).toHaveValue('150');
      await shot(alice, info, 'rejected');
    };

    const bobFlow = async () => {
      await bet(bob, 'b', 50);
      await expect(panel(bob).getByTestId('bet-mine')).toHaveText(
        t('bet_mine', { amount: coins(locale, 50), name: nameB }),
      );
    };

    const eveFlow = async () => {
      // Everything on the side that loses: then no coins, and the rescue waits for the settlement.
      const form = eve.getByTestId('bet-form');
      await expect(form).toBeVisible({ timeout: 20_000 });
      await press(form.getByTestId('bet-side-a'));
      await press(form.getByTestId('bet-amount-max'));
      await expect(form.getByTestId('bet-amount')).toHaveValue(String(GRANT));
      const answered = eve.waitForResponse((r) => r.url().includes('?/bet'));
      await press(form.getByTestId('bet-submit'));
      await answered;
      const broke = panel(eve).getByTestId('bet-broke');
      await expect(broke).toHaveAttribute('data-bailout', 'not_eligible');
      await expect(broke).toContainText(t('bet_broke_not_eligible'));
    };

    const carolFlow = async () => {
      await carol.goto(`/${locale}/`);
      const form = carol.getByTestId('bet-form');
      await expect(form).toBeVisible();
      const key = await form.locator('input[name="client_key"]').inputValue();
      await form.getByTestId('bet-side-a').locator('input').check();
      await form.getByTestId('bet-amount').fill('25');
      await form.getByTestId('bet-submit').click();
      await carol.waitForLoadState();
      await expect(carol.getByTestId('bet-mine')).toHaveText(
        t('bet_mine', { amount: coins(locale, 25), name: nameA }),
      );
      // The same form posted again bets nothing more (RF25 row 3).
      const again = await noJs.request.post(`/${locale}/?/bet`, {
        form: { fight_id: String(ko), side: 'a', amount: '25', client_key: key },
        headers: { origin: PUBLIC_ORIGIN },
      });
      expect(again.status()).toBeLessThan(400);
      const mine = await myBets(noJs.request, ko);
      expect([mine.staked, mine.bets.length]).toEqual([25, 1]);
    };

    await Promise.all([aliceFlow(), bobFlow(), eveFlow(), carolFlow()]);

    // A reload in the middle of the fight: the bet is still there (RF42, RF44).
    await untilLive(request, ko);
    await bob.reload();
    await waitForHydration(bob);
    await expect(bob.getByTestId('bet-placed')).toHaveText(
      t('bet_mine_closed', { amount: coins(locale, 50), name: nameB }),
    );

    // The result, once the view shows it, and the payout by reading again (RF36, RF45).
    const result = (page: Page) => page.getByTestId('bet-result');
    await expect(result(bob)).toHaveAttribute('data-outcome', 'won', { timeout: 150_000 });
    await expect(result(alice)).toHaveAttribute('data-outcome', 'lost', { timeout: 15_000 });
    await expect(result(alice)).toHaveText(t('bet_result_lost', { amount: coins(locale, 25) }));
    await expect(result(eve)).toHaveAttribute('data-outcome', 'lost', { timeout: 15_000 });
    await expect(bob.getByTestId('header-balance')).not.toHaveText(coins(locale, 50));
    await shot(bob, info, 'won');

    // The rescue, now that nothing is open.
    const rescue = eve.getByTestId('bet-bailout-claim');
    await expect(rescue).toBeVisible({ timeout: 15_000 });
    await press(rescue);
    await expect(eve.getByTestId('header-balance')).toHaveText(coins(locale, GRANT / 2));
    await Promise.all([eve.context().close(), noJs.close()]);

    // The draw: everything comes back.
    const draw = ko + 1;
    await untilOpen(request, draw);
    await Promise.all([bet(alice, 'a', 25), bet(bob, 'b', 25)]);
    for (const page of [alice, bob]) {
      await expect(result(page)).toHaveText(t('bet_result_draw', { amount: coins(locale, 25) }), {
        timeout: 150_000,
      });
    }

    // A KO where nobody bets on `b`: the notice before betting, and the refund after.
    const lonely = ko + 2;
    await untilOpen(request, lonely);
    const form = alice.getByTestId('bet-form');
    await expect(form).toBeVisible({ timeout: 20_000 });
    await expect(form.getByTestId('bet-one-sided')).toHaveText(t('bet_one_sided'));
    await Promise.all([bet(alice, 'a', 25), bet(bob, 'a', 25)]);
    for (const page of [alice, bob]) {
      await expect(result(page)).toHaveText(
        t('bet_result_one_sided', { amount: coins(locale, 25) }),
        {
          timeout: 150_000,
        },
      );
    }
  } finally {
    await Promise.all(contexts.map((context) => context.close().catch(() => undefined)));
  }
});
