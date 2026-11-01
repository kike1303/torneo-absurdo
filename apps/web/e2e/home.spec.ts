import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';
import type { FightEvent, FightSummary } from '@torneo/protocol';
import { readFileSync } from 'node:fs';
import {
  DELAY_MS,
  MAX_DURATION_MS,
  board,
  fakeServer,
  fight,
  koFight,
  liveAt,
  strikes,
  type FakeServer,
} from './support/fake-ws.js';
import { waitForHydration } from './support/hydration.js';
import { PUBLIC_ORIGIN } from './support/topology.js';

// 010 RF53 to RF58 and plan-d decision 15: every state of the view with a fake /ws. The general e2e
// has no roster, so the SSR always says "starting", and the first `schedule_update` brings the state
// to test. The real fights are in fights.spec.ts.
type Messages = Record<string, string>;
const load = (locale: string): Messages =>
  JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
const ES = load('es');
const EN = load('en');
const LONGEST_EPITHET = 'El Plomero que Todo lo Arregla Brincando';
const SHOTS = 'test-results/screenshots';
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;
const THEMES = ['light', 'dark'] as const;
const KO_AT = 30_000;
const LAST: FightSummary = fight(1, {
  state: 'settled',
  final: { kind: 'decided', winner: 'a', reason: 'decision' },
  a: { slug: 'rata-electrica', max_hp: 100 },
  b: { slug: 'ogro-del-pantano', max_hp: 100 },
});

const home = (page: Page) => page.getByTestId('fight-home');

async function open(page: Page, path = '/es/?debug=1'): Promise<void> {
  await page.goto(path);
  await waitForHydration(page);
}

function trackErrors(page: Page): string[] {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  return errors;
}

/** A session on a fight at `t`, with these events already released. */
async function watching(
  page: Page,
  t: number,
  events: readonly FightEvent[],
  over: Partial<FightSummary> = {},
): Promise<{ server: FakeServer; summary: FightSummary }> {
  const summary = liveAt(2, t, over);
  const server = await fakeServer(
    page,
    () => board({ current: summary }),
    (id, s) => {
      s.send({ type: 'fight_sync', fight: summary, server_time: String(Date.now()) });
      s.events(id, 0, events);
    },
  );
  await open(page);
  return { server, summary };
}

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

test.describe('the states of the view (RF54)', () => {
  test('without fights, with the real server: starting, in both languages', async ({ page }) => {
    await open(page);
    await expect(page.getByTestId('fight-starting')).toHaveText(ES.fight_view_starting as string);
    await expect(home(page)).toHaveAttribute('data-view', 'starting');
    await open(page, '/en/');
    await expect(page.getByTestId('fight-starting')).toHaveText(EN.fight_view_starting as string);
    // Without ?debug=1 nothing of the debug attributes.
    await expect(home(page)).not.toHaveAttribute('data-view', /./);
  });

  test('all null and halted null: loading, and it never says halted', async ({ page }) => {
    await fakeServer(page, () => board({ halted: null }));
    await open(page);
    await expect(page.getByTestId('fight-loading')).toBeVisible();
    await expect(page.getByTestId('fight-halted')).toHaveCount(0);
  });

  test('next: names, epithets and a countdown, with last next to it', async ({ page }) => {
    await fakeServer(page, () => board({ current: fight(2), last: LAST }));
    await open(page);
    await expect(page.getByTestId('fight-billboard')).toBeVisible();
    await expect(page.getByTestId('fight-billboard')).toContainText(LONGEST_EPITHET);
    await expect(page.getByTestId('fight-countdown')).toContainText(/\d:\d\d/);
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'decision');
    await expect(page.getByTestId('fight-result')).toContainText(ES.fight_view_last as string);
  });

  test('the countdown moves', async ({ page }) => {
    await fakeServer(page, () => board({ current: fight(2) }));
    await open(page);
    const countdown = page.getByTestId('fight-countdown');
    const first = await countdown.textContent();
    await expect(countdown).not.toHaveText(first ?? '', { timeout: 3000 });
  });

  test('an unknown slug has its fallback', async ({ page }) => {
    await fakeServer(page, () =>
      board({ current: fight(2, { a: { slug: 'striker', max_hp: 1 } }) }),
    );
    await open(page);
    await expect(page.getByTestId('fight-name-a')).toHaveText('Luchador striker');
  });

  test('the commit is closed, and whole and readable once open', async ({ page }) => {
    const commit = 'ef'.repeat(32);
    await page.setViewportSize({ width: 375, height: 667 });
    await fakeServer(page, () => board({ current: fight(2, { seed_commit: commit }) }));
    await open(page);
    const details = page.getByTestId('fight-commit');
    await expect(page.getByTestId('fight-commit-value')).not.toBeVisible();
    await details.locator('summary').click();
    await expect(page.getByTestId('fight-commit-value')).toHaveText(commit);
    await expect(page.getByTestId('fight-commit-value')).toBeVisible();
    expect(await overflow(page)).toBeLessThanOrEqual(0);
    const summary = await details.locator('summary').boundingBox();
    expect(summary?.height ?? 0).toBeGreaterThanOrEqual(44);
  });

  test('the countdown at zero without live: about to start, never a frozen 0:00', async ({
    page,
  }) => {
    const late = fight(2, {
      state: 'betting_closed',
      live_at: String(Date.now() - DELAY_MS - 1000),
    });
    await fakeServer(page, () => board({ current: late }));
    await open(page);
    await expect(page.getByTestId('fight-about-to-start')).toHaveText(
      ES.fight_view_about_to_start as string,
    );
    await expect(page.getByTestId('fight-countdown')).toHaveCount(0);
  });

  test('cancelled: "did not take place", in both languages', async ({ page }) => {
    const cancelled = fight(1, {
      state: 'settled',
      final: { kind: 'cancelled', reason: 'missed_window' },
    });
    await fakeServer(page, () => board({ last: cancelled }));
    await open(page);
    await expect(page.getByTestId('fight-result-text')).toHaveText(
      ES.fight_result_cancelled as string,
    );
    await open(page, '/en/');
    await expect(page.getByTestId('fight-result-text')).toHaveText(
      EN.fight_result_cancelled as string,
    );
  });

  test('halted: over about to start and over live, keeping last', async ({ page }) => {
    const current = liveAt(2, 1000);
    const server = await fakeServer(page, () => board({ halted: true, current, last: LAST }));
    await open(page);
    await expect(page.getByTestId('fight-halted')).toHaveText(ES.fight_view_halted as string);
    await expect(page.getByTestId('fight-halted')).toHaveAttribute('role', 'status');
    await expect(page.getByTestId('fight-live')).toHaveCount(0);
    await expect(page.getByTestId('fight-result')).toBeVisible();
    server.update(board({ halted: false, current, last: LAST }));
    await expect(page.getByTestId('fight-halted')).toHaveCount(0);
    // Its events have not arrived, so it waits for the first frame.
    await expect(page.getByTestId('fight-about-to-start')).toBeVisible();
  });

  test('the page is a 200 without a cache, and the socket brings the board', async ({ page }) => {
    const server = await fakeServer(page, () => board({ current: fight(2) }));
    const response = await page.goto('/es/');
    expect(response?.status()).toBe(200);
    expect(response?.headers()['cache-control']).toContain('no-store');
    await waitForHydration(page);
    await expect(page.getByTestId('fight-billboard')).toBeVisible();
    server.closeAll();
  });
});

test.describe('in live (RF54, RF55)', () => {
  test('live: the stage, the label and the clock, without a progress bar', async ({ page }) => {
    await watching(page, 10_000, strikes(10_000));
    await expect(page.getByTestId('fight-live-badge')).toHaveText(ES.fight_view_live as string);
    await expect(page.getByTestId('round-clock')).toHaveAttribute('role', 'timer');
    await expect(page.getByTestId('round-clock')).toHaveAttribute('aria-live', 'off');
    await expect(page.getByTestId('round-clock')).toContainText(/1:1\d/);
    await expect(page.locator('progress, [role="progressbar"]')).toHaveCount(0);
  });

  test('the end received does not show before its moment', async ({ page }) => {
    // The whole fight is out, as 3.5 s before it shows on screen; `t` is still before the closing.
    const { server, summary } = await watching(page, KO_AT - 3000, koFight(KO_AT));
    await expect(page.getByTestId('fight-live')).toBeVisible();
    await expect(page.getByTestId('fight-result')).toHaveCount(0);
    await expect(page.getByTestId('fight-live-badge')).toBeVisible();
    // Neither the board with its final, while it is being watched.
    const settled: FightSummary = {
      ...summary,
      state: 'settled',
      final: { kind: 'decided', winner: 'a', reason: 'ko' },
    };
    server.update(board({ last: settled }));
    await page.waitForTimeout(300);
    await expect(page.getByTestId('fight-result')).toHaveCount(0);
    const html = await home(page).innerText();
    expect(html).not.toContain(ES.fight_result_ko?.replace(' {name}', '') ?? '');
  });

  test('the clock stops at the ko, and only then', async ({ page }) => {
    await watching(page, KO_AT - 1500, koFight(KO_AT));
    const clock = page.getByTestId('round-clock');
    const seconds = Math.ceil((86_000 - KO_AT) / 1000);
    const frozen = `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, '0')}`;
    await expect(clock).not.toHaveText(frozen);
    await expect(clock).toHaveText(frozen, { timeout: 5000 });
    await page.waitForTimeout(1200);
    await expect(clock).toHaveText(frozen);
  });

  test('coming in past the closing, before Settled: the result, without any cue', async ({
    page,
  }) => {
    // RF54 against RF50: t beyond the closing, so `onCue` never fires it.
    await watching(page, KO_AT + 2000, koFight(KO_AT));
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko', {
      timeout: 15_000,
    });
    await expect(page.getByTestId('fight-live-badge')).toHaveCount(0);
  });

  test('coming in past end.t: no canvas at all, the result card', async ({ page }) => {
    await watching(page, KO_AT + 4500, koFight(KO_AT));
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko');
    await expect(home(page)).toHaveAttribute('data-stage', 'none');
    await page.waitForTimeout(500);
    await expect(page.locator('canvas')).toHaveCount(0);
  });

  test('the pose goes at end.t + 10 s, and the card stays', async ({ page }) => {
    await watching(page, KO_AT + 2000, koFight(KO_AT));
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko', {
      timeout: 15_000,
    });
    await expect(page.getByTestId('fight-live-badge')).toHaveCount(0);
    await expect(home(page)).toHaveAttribute('data-stage', 'visible');
    await expect(home(page)).toHaveAttribute('data-stage', 'none', { timeout: 20_000 });
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko');
  });

  test('a watched fight that becomes voided shows it at once', async ({ page }) => {
    const { server, summary } = await watching(page, 5000, strikes(5000));
    await expect(page.getByTestId('fight-live')).toBeVisible();
    const voided: FightSummary = {
      ...summary,
      state: 'settled',
      final: { kind: 'voided', reason: 'engine_error' },
    };
    server.update(board({ last: voided }));
    await expect(page.getByTestId('fight-result-text')).toHaveText(
      ES.fight_result_voided as string,
    );
    await expect(home(page)).toHaveAttribute('data-stage', 'none');
  });

  test('a late client far from its KO: the next fight does not take it, nor shows the result', async ({
    page,
  }) => {
    const { server, summary } = await watching(page, 12_000, koFight(KO_AT));
    await expect(home(page)).toHaveAttribute('data-stage', 'visible', { timeout: 15_000 });
    const settled: FightSummary = {
      ...summary,
      state: 'settled',
      final: { kind: 'decided', winner: 'a', reason: 'ko' },
    };
    const next = fight(3, { state: 'betting_closed' });
    server.update(board({ current: next, last: settled }));
    await page.waitForTimeout(400);
    await expect(page.getByTestId('fight-result')).toHaveCount(0);
    await expect(home(page)).toHaveAttribute('data-stage', 'visible');
    await expect(page.getByTestId('fight-live')).toBeVisible();
    expect(server.received()).not.toContainEqual({ type: 'subscribe', channel: 'fight:3' });
  });

  test('the next fight in pre_live waits for the pose: 10 s after end.t', async ({ page }) => {
    const { server, summary } = await watching(page, KO_AT + 3000, koFight(KO_AT));
    await expect(home(page)).toHaveAttribute('data-stage', 'visible', { timeout: 15_000 });
    const settled: FightSummary = {
      ...summary,
      state: 'settled',
      final: { kind: 'decided', winner: 'a', reason: 'ko' },
    };
    const next = fight(3, { state: 'betting_closed' });
    server.update(board({ current: next, last: settled }));
    await page.waitForTimeout(5000);
    await expect(home(page)).toHaveAttribute('data-stage', 'visible');
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko');
    await expect(page.getByTestId('fight-billboard')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko');
    await expect
      .poll(() => server.received())
      .toContainEqual({ type: 'subscribe', channel: 'fight:3' });
  });

  test('a fight_sync without events in live waits for the intro', async ({ page }) => {
    const summary = liveAt(2, -2000);
    const server = await fakeServer(
      page,
      () => board({ current: summary }),
      (_id, s) => s.send({ type: 'fight_sync', fight: summary, server_time: String(Date.now()) }),
    );
    await open(page);
    await expect(home(page)).toHaveAttribute('data-stage', 'hidden');
    await page.waitForTimeout(500);
    await expect(page.locator('canvas')).toHaveCount(0);
    await expect(page.getByTestId('fight-billboard')).toBeVisible();
    server.events(2, 0, [{ type: 'intro', t: 0 }]);
    await expect(home(page)).toHaveAttribute('data-events', '1');
  });

  test('pages that overlap, of any size, keep the events once', async ({ page }) => {
    const events = strikes(30_000);
    const summary = liveAt(2, 1000);
    const server = await fakeServer(
      page,
      () => board({ current: summary }),
      (_id, s) => s.send({ type: 'fight_sync', fight: summary, server_time: String(Date.now()) }),
    );
    await open(page);
    await expect.poll(() => server.received().length).toBeGreaterThan(1);
    server.events(2, 0, events.slice(0, 5));
    server.events(2, 3, events.slice(3, 9));
    server.events(2, 0, events.slice(0, 2));
    await expect(home(page)).toHaveAttribute('data-events', '9');
  });

  test('a gap in the pages reconnects, and the new connection fills it', async ({ page }) => {
    const events = strikes(30_000);
    const summary = liveAt(2, 1000);
    const server = await fakeServer(
      page,
      () => board({ current: summary }),
      (id, s) => {
        s.send({ type: 'fight_sync', fight: summary, server_time: String(Date.now()) });
        s.events(id, 0, events.slice(0, 3));
      },
    );
    await open(page);
    await expect(home(page)).toHaveAttribute('data-events', '3');
    const before = server.connections();
    server.events(2, 10, events.slice(10, 12));
    await expect(home(page)).toHaveAttribute('data-events', '3');
    await expect.poll(() => server.connections()).toBeGreaterThan(before);
    await expect(home(page)).toHaveAttribute('data-ws-failures', '1');
  });

  test('a burst of cues overdue after a cut: the scene is right, with no errors', async ({
    page,
  }) => {
    const errors = trackErrors(page);
    const { server } = await watching(page, 20_000, strikes(4000));
    await expect(page.getByTestId('fight-live')).toBeVisible();
    server.events(2, 0, strikes(20_000));
    await expect(home(page)).toHaveAttribute('data-events', String(strikes(20_000).length));
    expect(errors).toEqual([]);
    await expect(page.getByTestId('fight-result')).toHaveCount(0);
  });
});

test.describe('without the canvas', () => {
  const noWebGl = (page: Page) =>
    page.addInitScript(() => {
      const original = HTMLCanvasElement.prototype.getContext;
      HTMLCanvasElement.prototype.getContext = function (
        this: HTMLCanvasElement,
        type: string,
        ...rest: unknown[]
      ) {
        if (type.startsWith('webgl') || type === 'experimental-webgl') return null;
        return (original as (...args: unknown[]) => unknown).call(this, type, ...rest);
      } as unknown as typeof original;
    });

  test('without WebGL: live in text, with HP by t, and the result only from end.t', async ({
    page,
  }) => {
    await noWebGl(page);
    await watching(page, KO_AT + 3000, koFight(KO_AT));
    await expect(page.getByTestId('fight-notice-no-animation')).toBeVisible();
    await expect(page.getByTestId('fight-text-hp')).toBeVisible();
    await expect(page.getByTestId('fight-live')).toBeVisible();
    await expect(page.getByTestId('fight-result')).toHaveCount(0);
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko', {
      timeout: 5000,
    });
  });

  test('halted in text mode keeps the names and the HP', async ({ page }) => {
    await noWebGl(page);
    const { server, summary } = await watching(page, 5000, strikes(30_000));
    await expect(page.getByTestId('fight-text-hp')).toBeVisible();
    server.update(board({ halted: true, current: summary }));
    await expect(page.getByTestId('fight-halted')).toBeVisible();
    await expect(page.getByTestId('fight-text-hp')).toBeVisible();
    await expect(page.getByTestId('fight-live')).toBeVisible();
    await expect(page.getByTestId('fight-live-badge')).toHaveCount(0);
  });

  test('extend rejected in a later delivery: the notice, and the fight in text', async ({
    page,
    browserName,
  }) => {
    test.skip(browserName === 'webkit', 'needs WebGL in the first place');
    const errors = trackErrors(page);
    const { server } = await watching(page, 4000, strikes(5000));
    await expect(page.getByTestId('fight-live')).toBeVisible();
    await expect(home(page)).toHaveAttribute('data-stage', 'visible');
    // Strikes 100 ms apart go through the adapter and `validateTimeline` refuses them.
    const count = strikes(5000).length;
    server.events(2, count, [
      { type: 'attack', t: 5050, attacker: 'a', ability: 'x', kind: 'strike' },
      {
        type: 'hit',
        t: 5100,
        attacker: 'a',
        ability: 'x',
        damage: 1,
        crit: false,
        weak: false,
        hp: 97,
      },
    ]);
    await expect(page.getByTestId('fight-notice-playback')).toHaveText(
      ES.fight_notice_playback_error as string,
    );
    await expect(page.getByTestId('fight-text-hp')).toBeVisible();
    await expect(home(page)).toHaveAttribute('data-stage', 'none');
    expect(errors).toEqual([]);
  });

  test('extend rejected in the immediate delivery: the notice, and no canvas stays', async ({
    page,
  }) => {
    const bad: FightEvent[] = [
      ...strikes(5000),
      { type: 'attack', t: 5050, attacker: 'a', ability: 'x', kind: 'strike' },
      {
        type: 'hit',
        t: 5100,
        attacker: 'a',
        ability: 'x',
        damage: 1,
        crit: false,
        weak: false,
        hp: 97,
      },
    ];
    const summary = liveAt(2, 4000);
    const server = await fakeServer(
      page,
      () => board({ current: summary }),
      (id, s) => {
        s.send({ type: 'fight_sync', fight: summary, server_time: String(Date.now()) });
        s.events(id, 0, strikes(5000));
        // Right after it is up to date, while the sheets of the first load are on their way.
        setTimeout(() => s.events(id, 0, bad), 320);
      },
    );
    await open(page);
    await expect(page.getByTestId('fight-text-hp')).toBeVisible({ timeout: 15_000 });
    await expect(home(page)).toHaveAttribute('data-stage', 'none');
    expect(server.connections()).toBe(1);
  });

  test('events the adapter refuses: the notice, and the board goes on', async ({ page }) => {
    await watching(page, 4000, [
      { type: 'intro', t: 0 },
      { type: 'dodge', t: 3000, attacker: 'a', ability: 'x' },
    ]);
    await expect(page.getByTestId('fight-notice-playback')).toBeVisible({ timeout: 15_000 });
    await expect(page.getByTestId('fight-live')).toBeVisible();
  });
});

test.describe('the stage in pre_live', () => {
  test('the board stays until the first frame, and the hidden stage has a size', async ({
    page,
  }) => {
    const closed = fight(2, {
      state: 'betting_closed',
      live_at: String(Date.now() + 20_000),
    });
    await fakeServer(page, () => board({ current: closed }));
    await open(page);
    await expect(page.getByTestId('fight-billboard')).toBeVisible();
    await expect(home(page)).toHaveAttribute('data-stage', 'hidden');
    const box = await page.getByTestId('fight-stage').boundingBox();
    expect(box?.width ?? 0).toBeGreaterThan(100);
    expect(box?.height ?? 0).toBeGreaterThan(50);
    await expect(page.getByTestId('fight-stage')).not.toBeVisible();
  });

  test('with a limited mobile network the first frame arrives before t = 0', async ({
    page,
    browserName,
  }) => {
    test.skip(browserName !== 'chromium', 'CDP');
    test.setTimeout(60_000);
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Network.enable');
    await cdp.send('Network.emulateNetworkConditions', {
      offline: false,
      // The Fast 3G of DevTools.
      latency: 150 * 3.75,
      downloadThroughput: (1.6 * 1024 * 1024 * 0.9) / 8,
      uploadThroughput: (750 * 1024 * 0.9) / 8,
    });
    const closed = fight(2, { state: 'betting_closed', live_at: String(Date.now() + 25_000) });
    const server = await fakeServer(page, () => board({ current: closed }));
    await open(page);
    await cdp.send('Emulation.setCPUThrottlingRate', { rate: 4 });
    await expect(home(page)).toHaveAttribute('data-stage', 'hidden');
    // pre_live goes by, as it does on the server: 30 s by default, and here what is left of 25 s.
    const startsAt = Number(closed.live_at);
    await page.waitForTimeout(Math.max(0, startsAt - Date.now()));
    const live = { ...closed, state: 'live' as const, live_started_at: String(Date.now()) };
    server.update(board({ current: live }));
    server.events(2, 0, [{ type: 'intro', t: 0 }]);
    await expect(home(page)).toHaveAttribute('data-painted-t', /-?\d/, { timeout: 30_000 });
    const paintedT = Number(await home(page).getAttribute('data-painted-t'));
    test.info().annotations.push({ type: 'painted_t_ms', description: String(paintedT) });
    expect(paintedT).toBeLessThan(0);
  });
});

test.describe('reconnection and capacity (RF57)', () => {
  test('with the WS blocked: one notice, a poll, and no round clock', async ({ page }) => {
    const current = liveAt(2, 5000);
    let polls = 0;
    await page.route('**/api/fights/current', async (route) => {
      polls++;
      await route.fulfill({ json: board({ current }) });
    });
    const server = await fakeServer(page, () => board({ current }));
    server.block();
    await open(page);
    const notice = page.getByTestId('fight-notice-degraded');
    await expect(notice).toHaveText(ES.fight_notice_degraded as string, { timeout: 20_000 });
    await expect(notice).toHaveCount(1);
    await expect(notice).toHaveAttribute('role', 'status');
    await expect(home(page)).toHaveAttribute('data-connection', 'degraded');
    await expect.poll(() => polls, { timeout: 5000 }).toBeGreaterThan(0);
    await expect(page.getByTestId('fight-live')).toBeVisible();
    await expect(page.getByTestId('round-clock')).toHaveCount(0);
    server.allow();
    await expect(notice).toHaveCount(0, { timeout: 40_000 });
    await expect(home(page)).toHaveAttribute('data-connection', 'live');
  });

  test('degraded still shows a result, from the poll', async ({ page }) => {
    await page.route('**/api/fights/current', (route) =>
      route.fulfill({ json: board({ last: LAST }) }),
    );
    const server = await fakeServer(page);
    server.block();
    await open(page);
    await expect(page.getByTestId('fight-result')).toHaveAttribute('data-result', 'decision', {
      timeout: 20_000,
    });
    await expect(page.getByTestId('fight-notice-degraded')).toBeVisible();
  });

  for (const [name, fire] of [
    ['online', (p: Page) => p.evaluate(() => window.dispatchEvent(new Event('online')))],
    [
      'visibilitychange',
      (p: Page) => p.evaluate(() => document.dispatchEvent(new Event('visibilitychange'))),
    ],
  ] as const) {
    test(`${name} with the socket cut reconnects at once`, async ({ page }) => {
      const server = await fakeServer(page);
      await open(page);
      await expect.poll(() => server.received().length).toBeGreaterThan(0);
      // Far in the backoff: the next attempt would take seconds.
      server.block();
      await expect.poll(() => server.connections()).toBeGreaterThan(2);
      server.allow();
      const before = server.connections();
      await page.waitForTimeout(2100);
      await fire(page);
      await expect.poll(() => server.connections(), { timeout: 1500 }).toBeGreaterThan(before);
    });
  }

  test('visibilitychange with the socket alive: a probe on schedule, and a sample of the hour', async ({
    page,
  }) => {
    let polls = 0;
    await page.route('**/api/fights/current', async (route) => {
      polls++;
      await route.fulfill({ json: board() });
    });
    const server = await fakeServer(page);
    await open(page);
    await expect.poll(() => server.received().length).toBeGreaterThan(0);
    const count = server.received().length;
    await page.waitForTimeout(2100);
    await page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
    await expect.poll(() => server.received().length).toBeGreaterThan(count);
    expect(server.received().at(-1)).toEqual({ type: 'subscribe', channel: 'schedule' });
    await expect.poll(() => polls).toBeGreaterThan(0);
  });
});

test('webglcontextlost: it draws again at its t', async ({ page, browserName }) => {
  test.skip(browserName !== 'chromium', 'WEBGL_lose_context');
  await watching(page, 6000, strikes(30_000));
  await expect(home(page)).toHaveAttribute('data-stage', 'visible', { timeout: 15_000 });
  const stage = page.getByTestId('fight-stage');
  await expect(stage).toHaveAttribute('data-state', 'playing');
  const lost = await page.evaluate(() => {
    const canvas = document.querySelector('canvas');
    const gl = canvas?.getContext('webgl2') ?? canvas?.getContext('webgl');
    const extension = gl?.getExtension('WEBGL_lose_context');
    extension?.loseContext();
    return extension !== null && extension !== undefined;
  });
  expect(lost).toBe(true);
  await expect(stage).toHaveAttribute('data-state', 'playing', { timeout: 15_000 });
  await expect(page.getByTestId('fight-notice-no-animation')).toHaveCount(0);
  await expect(page.locator('canvas')).toHaveCount(1);
});

test.describe('375 px', () => {
  for (const locale of ['es', 'en'] as const) {
    test(`the billboard fits whole in 375×667, ${locale}, with the longest epithet`, async ({
      page,
    }) => {
      await page.setViewportSize({ width: 375, height: 667 });
      await fakeServer(page, () => board({ current: fight(2, { seed_commit: 'ab'.repeat(32) }) }));
      await open(page, `/${locale}/?debug=1`);
      const card = page.getByTestId('fight-billboard');
      await expect(card).toBeVisible();
      const box = await card.boundingBox();
      expect((box?.y ?? 0) + (box?.height ?? 0)).toBeLessThanOrEqual(667);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
      await expect(page.getByTestId('fight-countdown')).toHaveCSS(
        'font-variant-numeric',
        'tabular-nums',
      );
    });
  }
});

// Artifacts for review, not pixel comparisons, and axe in every state (RF58).
type State = {
  readonly name: string;
  readonly set: (page: Page) => Promise<void>;
  readonly ready: (page: Page) => Promise<void>;
};

const STATES: readonly State[] = [
  {
    name: 'loading',
    set: async (page) => {
      await fakeServer(page, () => board({ halted: null }));
    },
    ready: (page) => expect(page.getByTestId('fight-loading')).toBeVisible(),
  },
  {
    name: 'starting',
    set: async () => undefined,
    ready: (page) => expect(page.getByTestId('fight-starting')).toBeVisible(),
  },
  {
    name: 'next',
    set: async (page) => {
      await fakeServer(page, () =>
        board({ current: fight(2, { seed_commit: 'ab'.repeat(32) }), last: LAST }),
      );
    },
    ready: (page) => expect(page.getByTestId('fight-billboard')).toBeVisible(),
  },
  {
    name: 'about-to-start',
    set: async (page) => {
      const late = fight(2, { state: 'betting_closed', live_at: String(Date.now() - 5000) });
      await fakeServer(page, () => board({ current: late, last: LAST }));
    },
    ready: (page) => expect(page.getByTestId('fight-about-to-start')).toBeVisible(),
  },
  {
    name: 'live',
    set: async (page) => {
      const summary = liveAt(2, 11_000);
      await fakeServer(
        page,
        () => board({ current: summary }),
        (id, s) => {
          s.send({ type: 'fight_sync', fight: summary, server_time: String(Date.now()) });
          s.events(id, 0, strikes(30_000));
        },
      );
    },
    ready: (page) => expect(page.getByTestId('fight-live')).toBeVisible({ timeout: 15_000 }),
  },
  {
    name: 'result',
    set: async (page) => {
      await fakeServer(page, () => board({ last: LAST }));
    },
    ready: (page) => expect(page.getByTestId('fight-result')).toBeVisible(),
  },
  {
    name: 'cancelled',
    set: async (page) => {
      const last = fight(1, {
        state: 'settled',
        final: { kind: 'cancelled', reason: 'missed_window' },
      });
      await fakeServer(page, () => board({ current: fight(2), last }));
    },
    ready: (page) => expect(page.getByTestId('fight-result')).toBeVisible(),
  },
  {
    name: 'voided',
    set: async (page) => {
      const last = fight(1, {
        state: 'settled',
        final: { kind: 'voided', reason: 'engine_error' },
      });
      await fakeServer(page, () => board({ current: fight(2), last }));
    },
    ready: (page) => expect(page.getByTestId('fight-result')).toBeVisible(),
  },
  {
    name: 'halted',
    set: async (page) => {
      await fakeServer(page, () => board({ halted: true, current: fight(2), last: LAST }));
    },
    ready: (page) => expect(page.getByTestId('fight-halted')).toBeVisible(),
  },
  {
    name: 'degraded',
    set: async (page) => {
      await page.route('**/api/fights/current', (route) =>
        route.fulfill({ json: board({ current: fight(2) }) }),
      );
      const server = await fakeServer(page);
      server.block();
    },
    ready: (page) =>
      expect(page.getByTestId('fight-notice-degraded')).toBeVisible({ timeout: 20_000 }),
  },
];

for (const state of STATES) {
  for (const theme of THEMES) {
    test(`axe in ${state.name}, ${theme}`, async ({ page }) => {
      await page.context().addCookies([{ name: 'theme', value: theme, url: PUBLIC_ORIGIN }]);
      await state.set(page);
      await open(page);
      await state.ready(page);
      const results = await new AxeBuilder({ page })
        .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
        .analyze();
      const serious = results.violations.filter(
        (v) => v.impact === 'serious' || v.impact === 'critical',
      );
      expect(serious.map((v) => v.id)).toEqual([]);
    });

    for (const locale of ['es', 'en'] as const) {
      for (const size of SIZES) {
        test(`screenshot ${state.name} ${locale} ${size.name} ${theme}`, async ({
          page,
          browserName,
        }) => {
          test.skip(browserName !== 'chromium', 'the artifacts are of one engine');
          await page.context().addCookies([{ name: 'theme', value: theme, url: PUBLIC_ORIGIN }]);
          await page.setViewportSize(size);
          await state.set(page);
          await open(page, `/${locale}/?debug=1`);
          await state.ready(page);
          await page.waitForTimeout(300);
          expect(await overflow(page)).toBeLessThanOrEqual(0);
          await page.screenshot({
            path: `${SHOTS}/home-${state.name}-${locale}-${size.name}-${theme}.png`,
            fullPage: true,
          });
        });
      }
    }
  }
}

test('MAX_DURATION_MS of the fake server is the one of the engine', () => {
  expect(MAX_DURATION_MS).toBe(90_000);
});
