import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import {
  expect,
  test,
  type APIRequestContext,
  type Browser,
  type BrowserContext,
  type Page,
  type TestInfo,
} from '@playwright/test';
import type { FightDetail, FightSummary, FightsCurrent } from '@torneo/protocol';
import { VECTORS, goldenLog } from './support/fight-script.mjs';
import { login } from './auth/fixtures.js';
import { SERVER_LOG } from './support/logs.js';
import { EGRESS_PROXY, PUBLIC_ORIGIN } from './support/topology.js';

// The real fights (010 preocupaciones 9 and 10, plan-d decisions 16 and 17): one flow per project,
// with simultaneous contexts, on the same fights as the other projects. A fight is told by its slugs,
// never by its order, because RF45 picks the row by the rows before it.
const RUN_LIMIT_MS = 7.5 * 60_000;
const SYNC_TOLERANCE_MS = 500;
const SYNC_SAMPLES = 8;
const TIME_UP_MS = 86_000;
const PROJECTS = ['chromium', 'webkit', 'chromium-mobile', 'webkit-mobile'];

type FightKind = keyof typeof VECTORS;

const home = (page: Page) => page.getByTestId('fight-home');
const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, Math.max(0, ms)));

async function boardOf(request: APIRequestContext): Promise<FightsCurrent> {
  const response = await request.get('/api/fights/current');
  expect(response.status()).toBe(200);
  return (await response.json()) as FightsCurrent;
}

const isKind = (fight: FightSummary | null, kind: FightKind): fight is FightSummary =>
  fight !== null && fight.a.slug === VECTORS[kind].a && fight.b.slug === VECTORS[kind].b;

/** The first fight of these slugs that has not started on screen when it is asked for. */
async function nextOf(request: APIRequestContext, kind: FightKind): Promise<number> {
  for (;;) {
    const board = await boardOf(request);
    for (const fight of [board.current, board.next]) {
      if (!isKind(fight, kind)) continue;
      const started =
        fight.live_started_at !== null &&
        Number(board.server_time) >= Number(fight.live_started_at) + fight.playback_delay_ms;
      if (!started) return fight.id;
    }
    await sleep(500);
  }
}

/** Waits until `fight` has started, and gives the fixed hour of its start. */
async function startOf(request: APIRequestContext, id: number): Promise<number> {
  for (;;) {
    const { current } = await boardOf(request);
    if (current?.id === id && current.live_started_at !== null) {
      return Number(current.live_started_at);
    }
    await sleep(200);
  }
}

/** Until the server's hour is `at`, whatever the clock of this machine says. */
async function untilServerTime(request: APIRequestContext, at: number): Promise<void> {
  for (;;) {
    const left = at - Number((await boardOf(request)).server_time);
    if (left <= 0) return;
    await sleep(Math.min(left, 1000));
  }
}

/** Upgrades are limited by IP, and this comes from one: one entry per project, a step apart. */
async function turn(info: TestInfo, stepMs = 1000): Promise<void> {
  await sleep(PROJECTS.indexOf(info.project.name) * stepMs);
}

async function newContext(browser: Browser, info: TestInfo): Promise<BrowserContext> {
  const use = info.project.use;
  return browser.newContext({
    baseURL: PUBLIC_ORIGIN,
    proxy: EGRESS_PROXY,
    viewport: use.viewport,
    userAgent: use.userAgent,
    deviceScaleFactor: use.deviceScaleFactor,
    isMobile: use.isMobile,
    hasTouch: use.hasTouch,
  });
}

async function watch(context: BrowserContext, locale: 'es' | 'en'): Promise<Page> {
  const page = await context.newPage();
  await page.goto(`/${locale}/?debug=1`);
  return page;
}

async function liveSample(page: Page): Promise<number> {
  const [t, at] = await Promise.all([
    home(page).getAttribute('data-t'),
    home(page).getAttribute('data-t-at'),
  ]);
  return Number(t) - Number(at);
}

/** The connection never went degraded, and no socket failed, the whole run (RF57). */
async function neverDegraded(page: Page): Promise<void> {
  await expect(home(page)).toHaveAttribute('data-connection', 'live');
  await expect(home(page)).toHaveAttribute('data-ws-failures', '0');
}

async function subscribes(page: Page): Promise<number> {
  return Number(await home(page).getAttribute('data-subscribes'));
}

async function verify(request: APIRequestContext, id: number, kind: FightKind): Promise<void> {
  await expect
    .poll(async () => ((await boardOf(request)).current?.id === id ? 'running' : 'settled'), {
      timeout: 120_000,
    })
    .toBe('settled');
  const response = await request.get(`/api/fights/${id}`);
  expect(response.status()).toBe(200);
  const detail = (await response.json()) as FightDetail;
  expect(detail.log).toBe(goldenLog(kind));
  const seed = Buffer.from(detail.seed ?? '', 'hex');
  expect(createHash('sha256').update(seed).digest('hex')).toBe(detail.fight.seed_commit);
}

test('the same real fights in every project', async ({ browser, request }, info) => {
  test.setTimeout(RUN_LIMIT_MS);
  const contexts: BrowserContext[] = [];
  const opened = async (): Promise<BrowserContext> => {
    const context = await newContext(browser, info);
    contexts.push(context);
    return context;
  };

  /** A KO: next, live, its result, and the result next to the next fight. */
  const koFlow = async (): Promise<void> => {
    const id = await nextOf(request, 'ko');
    await turn(info);
    const main = await watch(await opened(), 'es');
    await expect(home(main)).toHaveAttribute('data-view', /^(next|about_to_start)$/);
    await expect(main.getByTestId('fight-billboard')).toBeVisible();
    await expect(main.getByTestId('fight-name-a')).toHaveText('Luchador striker');

    const start = await startOf(request, id);
    await expect(home(main)).toHaveAttribute('data-view', 'live', { timeout: 30_000 });
    await expect(main.getByTestId('fight-live-badge')).toBeVisible();

    // B: coming in at 20 s of `t`.
    await untilServerTime(request, start + 3500 + 20_000);
    await turn(info);
    const late = await watch(await opened(), 'en');
    await expect(home(late)).toHaveAttribute('data-view', 'live', { timeout: 30_000 });

    await expect(main.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko', {
      timeout: 90_000,
    });
    await expect(main.getByTestId('fight-live-badge')).toHaveCount(0);
    await expect(main.getByTestId('fight-result-text')).toHaveText(
      'Luchador trickster ganó por KO',
    );
    await expect(late.getByTestId('fight-result-text')).toHaveText('Fighter trickster won by KO', {
      timeout: 15_000,
    });

    // Both received the same events in the same order.
    await expect(home(late)).toHaveAttribute(
      'data-event-digest',
      (await home(main).getAttribute('data-event-digest')) ?? '',
      { timeout: 15_000 },
    );
    // The pose goes, and the result stays next to what comes.
    await expect(main.getByTestId('fight-billboard')).toBeVisible({ timeout: 30_000 });
    await expect(main.getByTestId('fight-result')).toHaveAttribute('data-result', 'ko');
    await verify(request, id, 'ko');
    await neverDegraded(main);
    await neverDegraded(late);
  };

  /** A draw: two contexts in sync, a cut, and coming in past its closing. */
  const drawFlow = async (): Promise<void> => {
    const id = await nextOf(request, 'draw');
    // Each group 10 s from the other: the upgrades come back meanwhile.
    await turn(info);
    const sessionContext = await opened();
    const sessionPage = await sessionContext.newPage();
    // A first login goes through the onboarding: the view comes after it, by address.
    await login(sessionPage, `e2e-fights-${info.project.name}`, 'es');
    await sessionPage.goto('/es/?debug=1');
    await sleep(10_000);
    await turn(info);
    const anon = await watch(await opened(), 'en');
    await sleep(10_000);
    await turn(info);
    const cutContext = await opened();
    let cuts = 0;
    let closeSocket: () => void = () => undefined;
    await cutContext.routeWebSocket('**/ws', (socket) => {
      socket.connectToServer();
      if (cuts === 0) closeSocket = () => void socket.close();
      cuts++;
    });
    const cut = await watch(cutContext, 'es');

    const start = await startOf(request, id);
    for (const page of [sessionPage, anon, cut]) {
      await expect(home(page)).toHaveAttribute('data-view', 'live', { timeout: 30_000 });
    }

    // RNF "Sincronización": with and without a session, within 500 ms.
    for (let i = 0; i < SYNC_SAMPLES; i++) {
      const [one, two] = await Promise.all([liveSample(sessionPage), liveSample(anon)]);
      expect(Math.abs(one - two), `sample ${i}`).toBeLessThanOrEqual(SYNC_TOLERANCE_MS);
      await sleep(1000);
    }

    // A cut at 30 s of `t`: the new connection catches up with pages.
    await untilServerTime(request, start + 3500 + 30_000);
    closeSocket();
    await expect.poll(() => cuts, { timeout: 15_000 }).toBeGreaterThan(1);

    // F: coming in past the closing, before Settled.
    await untilServerTime(request, start + TIME_UP_MS + 5000);
    await turn(info, 500);
    const window = await watch(await opened(), 'es');
    await expect(window.getByTestId('fight-result')).toHaveAttribute('data-result', 'draw', {
      timeout: 15_000,
    });

    await expect(anon.getByTestId('fight-result-text')).toHaveText('Draw', { timeout: 30_000 });
    await expect(sessionPage.getByTestId('fight-result-text')).toHaveText('Empate', {
      timeout: 15_000,
    });
    await expect(cut.getByTestId('fight-result-text')).toHaveText('Empate', { timeout: 15_000 });

    const digest = (await home(anon).getAttribute('data-event-digest')) ?? '';
    await expect(home(sessionPage)).toHaveAttribute('data-event-digest', digest);
    await expect(home(cut)).toHaveAttribute('data-event-digest', digest, { timeout: 15_000 });

    await verify(request, id, 'draw');
    for (const page of [sessionPage, anon, cut, window]) await neverDegraded(page);
    // DP-B7: one `fight_sync` asked for per fight and socket. The fights of this page are few.
    expect(await subscribes(sessionPage)).toBeLessThanOrEqual(3);
    expect(await subscribes(anon)).toBeLessThanOrEqual(3);
  };

  try {
    await Promise.all([koFlow(), drawFlow()]);
    // The real characters loaded, whatever the script does with them.
    expect(readFileSync(SERVER_LOG, 'utf8')).not.toContain('fights.roster_invalid');
  } finally {
    await Promise.all(contexts.map((context) => context.close()));
  }
});
