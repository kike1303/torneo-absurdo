import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';
import type { FightDetail, FightFinal } from '@torneo/protocol';
import { waitForHydration } from './support/hydration.js';
import { PUBLIC_ORIGIN } from './support/topology.js';

// 011, with the fights of e2e/support/db.mjs (`REPLAY_ROWS`, ids in that order): real specs and logs,
// and no roster. WebKit on Linux has no WebGL: there each flow goes through the branch without
// animation and is annotated `limited` (Enmienda plan: D3), so the controls in WebKit are read from
// these annotations when it runs on a machine that has it.
const ID = { ko: 1, decision: 2, draw: 3, voided: 4, missed: 5, unrunnable: 6, scheduled: 7 };
const SHOTS = 'test-results/screenshots';
const SIZES = [
  { name: 'mobile', width: 375, height: 812 },
  { name: 'desktop', width: 1280, height: 800 },
] as const;

type Messages = Record<string, string>;
const messages = (locale: string): Messages =>
  JSON.parse(readFileSync(new URL(`../messages/${locale}.json`, import.meta.url), 'utf8'));
const ES = messages('es');

const verdict = (page: Page) => page.getByTestId('replay-verdict');
const stage = (page: Page) => page.getByTestId('fight-stage');
const result = (page: Page) => page.getByTestId('fight-result');

const overflow = (page: Page) =>
  page.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);

async function noWebGl(page: Page): Promise<void> {
  await page.addInitScript(() => {
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
}

/** Whether the stage came up, paused, or this engine has no WebGL. */
async function stageUp(page: Page): Promise<boolean> {
  const toggle = page.getByTestId('replay-toggle');
  const notice = page.getByTestId('fight-notice-no-animation');
  await expect(toggle.or(notice)).toBeVisible({ timeout: 30_000 });
  await expect
    .poll(async () => (await notice.isVisible()) || (await toggle.isEnabled()), { timeout: 30_000 })
    .toBe(true);
  if (await notice.isVisible()) {
    test.info().annotations.push({ type: 'limited', description: 'no WebGL in this engine' });
    return false;
  }
  return true;
}

/** Fulfills the browser's read of the fight with this body, without spending the quota. */
async function answered(page: Page, id: number, body: unknown): Promise<void> {
  await page.route(`**/api/fights/${id}`, (route) => route.fulfill({ json: body }));
}

const sha256 = (text: string) => createHash('sha256').update(text).digest('hex');

test.describe('the replays', { tag: '@replays' }, () => {
  let ko: FightDetail;

  test.beforeAll(async ({ request }) => {
    const response = await request.get(`/api/fights/${ID.ko}`);
    expect(response.status()).toBe(200);
    ko = (await response.json()) as FightDetail;
  });

  test.describe('the list (RF17, RF20)', () => {
    test('only the settled, the most recent first, without the result', async ({ page }) => {
      await page.goto('/es/fights');
      await waitForHydration(page);
      const rows = page.getByTestId('fight-list-row');
      await expect(rows).toHaveCount(6);
      expect(
        await rows.evaluateAll((all) => all.map((row) => row.getAttribute('data-fight'))),
      ).toEqual(['6', '5', '4', '3', '2', '1']);
      await expect(page.getByTestId('fight-list-kind')).toHaveText([
        ES.replay_list_kind_cancelled ?? '',
        ES.replay_list_kind_cancelled ?? '',
        ES.replay_list_kind_voided ?? '',
        ES.replay_list_kind_played ?? '',
        ES.replay_list_kind_played ?? '',
        ES.replay_list_kind_played ?? '',
      ]);
      // RF25: the fighters of the goldens have no lore, and they keep their fallback.
      await expect(rows.last()).toContainText('Luchador striker');
      const text = (await page.getByTestId('fight-list').innerText()).toLowerCase();
      for (const spoiler of ['ganó', 'empate', ' ko', 'decisión', 'winner']) {
        expect(text).not.toContain(spoiler);
      }
      await expect(page.getByTestId('fight-list-more')).toHaveCount(0);
      expect(await overflow(page)).toBeLessThanOrEqual(0);
    });

    test('a row leads to its fight', async ({ page }) => {
      await page.goto('/en/fights');
      await waitForHydration(page);
      await page.locator(`[data-fight="${ID.draw}"]`).click();
      await page.waitForURL(`/en/fights/${ID.draw}`);
      await expect(page.getByTestId('fight-page')).toHaveAttribute('data-status', 'settled');
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
    });
  });

  test.describe('nothing before it is asked for (RF18, RF20)', () => {
    test('the HTML has no final, seed, log nor specs', async ({ request }) => {
      const html = await (await request.get(`/es/fights/${ID.ko}`)).text();
      expect(html).toContain('Luchador striker');
      for (const leak of [ko.seed ?? '-', ko.log ?? '-', '"winner"', 'end_reason', 'spec_a']) {
        expect(html).not.toContain(leak);
      }
    });

    test('the title has who fights, and the page does not say who won', async ({ page }) => {
      await page.goto(`/es/fights/${ID.ko}`);
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
      await expect(page).toHaveTitle(/Luchador striker vs Luchador trickster/);
      expect(await page.title()).not.toMatch(/ganó|KO|Empate/);
      await expect(result(page)).toHaveCount(0);
      const shown = await page.getByTestId('fight-page').innerText();
      expect(shown).not.toMatch(/ganó|Empate/);
      await expect(page.locator('progress, [role="progressbar"]')).toHaveCount(0);
    });
  });

  test.describe('watching it (RF19)', () => {
    for (const [name, id, ending] of [
      ['a KO', ID.ko, 'ko'],
      ['a decision', ID.decision, 'decision'],
      ['a draw', ID.draw, 'draw'],
    ] as const) {
      test(`${name}: to the end, again, back and forward`, async ({ page }) => {
        await page.goto(`/es/fights/${id}`);
        await waitForHydration(page);
        await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
        await expect(result(page)).toHaveCount(0);
        if (!(await stageUp(page))) {
          await page.getByTestId('replay-show-result').click();
          await expect(result(page)).toHaveAttribute('data-result', ending);
          return;
        }

        const toggle = page.getByTestId('replay-toggle');
        const forward = page.getByTestId('replay-forward');
        // Paused at t = 0, with the clock of the round.
        await expect(stage(page)).toHaveAttribute('data-state', 'ready');
        await expect(toggle).toHaveAttribute('data-action', 'play');
        await expect(page.getByTestId('round-clock')).toHaveText('1:26');

        // The jumps keep the state.
        await toggle.click();
        await expect(stage(page)).toHaveAttribute('data-state', 'playing');
        await expect(toggle).toHaveAttribute('data-action', 'pause');
        await forward.click();
        await expect(stage(page)).toHaveAttribute('data-state', 'playing');
        await toggle.click();
        await expect(stage(page)).toHaveAttribute('data-state', 'paused');
        await forward.click();
        await expect(stage(page)).toHaveAttribute('data-state', 'paused');

        // +10 s stays on until the very end.
        for (let i = 0; i < 12; i++) {
          if ((await stage(page).getAttribute('data-state')) === 'finished') break;
          await expect(forward).toBeEnabled();
          await forward.click();
        }
        await expect(stage(page)).toHaveAttribute('data-state', 'finished');
        await expect(toggle).toHaveAttribute('data-action', 'again');
        await expect(forward).toBeDisabled();
        await expect(result(page)).toHaveAttribute('data-result', ending);

        // Back from the end stays paused there, and playing reaches the end by itself.
        await page.getByTestId('replay-back').click();
        await expect(stage(page)).toHaveAttribute('data-state', 'paused');
        await toggle.click();
        await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 20_000 });

        // Restart goes to 0 and plays, and the result seen stays.
        await page.getByTestId('replay-restart').click();
        await expect(stage(page)).toHaveAttribute('data-state', 'playing');
        await expect(result(page)).toBeVisible();

        // "Watch again" too, and the end button leaves `finished`.
        await page.getByTestId('replay-end').click();
        await expect(stage(page)).toHaveAttribute('data-state', 'finished');
        await toggle.click();
        await expect(stage(page)).toHaveAttribute('data-state', 'playing');
      });
    }

    test('the controls fit, with 44 px targets', async ({ page }) => {
      await page.setViewportSize({ width: 375, height: 812 });
      await page.goto(`/en/fights/${ID.ko}`);
      if (!(await stageUp(page))) return;
      for (const name of ['toggle', 'back', 'forward', 'restart', 'end']) {
        const box = await page.getByTestId(`replay-${name}`).boundingBox();
        expect(box?.height, name).toBeGreaterThanOrEqual(44);
        expect(box?.width, name).toBeGreaterThanOrEqual(44);
      }
      expect(await overflow(page)).toBeLessThanOrEqual(0);
    });

    test('with reduced motion the controls still work (RF21)', async ({ page }) => {
      await page.emulateMedia({ reducedMotion: 'reduce' });
      await page.goto(`/es/fights/${ID.ko}`);
      await waitForHydration(page);
      if (!(await stageUp(page))) return;
      await page.getByTestId('replay-toggle').click();
      await expect(stage(page)).toHaveAttribute('data-state', 'playing');
      await page.getByTestId('replay-end').click();
      await expect(stage(page)).toHaveAttribute('data-state', 'finished');
      await expect(result(page)).toHaveAttribute('data-result', 'ko');
    });

    test('a lost context draws again at its t, paused (RF21)', async ({ page, browserName }) => {
      test.skip(browserName !== 'chromium', 'WEBGL_lose_context');
      await page.goto(`/es/fights/${ID.draw}`);
      await waitForHydration(page);
      if (!(await stageUp(page))) return;
      await page.getByTestId('replay-forward').click();
      await page.getByTestId('replay-forward').click();
      const clock = page.getByTestId('round-clock');
      await expect(clock).toHaveText('1:06');
      const lost = await page.evaluate(() => {
        const canvas = document.querySelector('canvas');
        const gl = canvas?.getContext('webgl2') ?? canvas?.getContext('webgl');
        const extension = gl?.getExtension('WEBGL_lose_context');
        extension?.loseContext();
        return extension !== null && extension !== undefined;
      });
      expect(lost).toBe(true);
      await expect(page.locator('canvas')).toHaveCount(1, { timeout: 15_000 });
      await expect(page.getByTestId('replay-toggle')).toBeEnabled({ timeout: 15_000 });
      await expect(clock).toHaveText('1:06');
      await expect(page.getByTestId('fight-notice-no-animation')).toHaveCount(0);
    });
  });

  test.describe('without lights out (RF21)', () => {
    test('without WebGL: no controls, the result by the button, the whole check', async ({
      page,
    }) => {
      await noWebGl(page);
      await page.goto(`/es/fights/${ID.ko}`);
      await expect(page.getByTestId('fight-notice-no-animation')).toBeVisible({ timeout: 30_000 });
      await expect(page.getByTestId('replay-controls')).toHaveCount(0);
      await expect(page.locator('canvas')).toHaveCount(0);
      await expect(result(page)).toHaveCount(0);
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
      await page.getByTestId('replay-show-result').click();
      await expect(result(page)).toHaveAttribute('data-result', 'ko');
    });

    test('with the wasm blocked: the fight plays, and a retry can pass', async ({ page }) => {
      await page.route('**/*.wasm', (route) => route.abort());
      await page.goto(`/es/fights/${ID.ko}`);
      await waitForHydration(page);
      await expect(verdict(page)).toHaveAttribute('data-status', 'cannot_check', {
        timeout: 30_000,
      });
      await expect(page.getByTestId('replay-verdict-text')).toHaveText(
        ES.replay_verify_cannot_check ?? '',
      );
      await stageUp(page);
      await page.unroute('**/*.wasm');
      await page.getByTestId('replay-verdict-retry').click();
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
    });
  });

  test.describe('the check (RF12 to RF16)', () => {
    test('a real fight matches, says its limit, and keeps the rest closed', async ({ page }) => {
      await page.goto(`/en/fights/${ID.ko}`);
      await waitForHydration(page);
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
      await expect(page.getByTestId('replay-verdict-limit')).toBeVisible();
      const more = page.getByTestId('replay-verdict-more');
      await expect(more).not.toHaveAttribute('open', '');
      await more.locator('summary').click();
      await expect(page.getByTestId('replay-field-seed')).toContainText(ko.seed ?? '-');
      await expect(page.getByTestId('replay-field-commit')).toContainText(
        ko.fight.seed_commit ?? '-',
      );
      await expect(page.getByTestId('replay-field-snapshot')).toContainText(
        ko.fight.snapshot_sha256,
      );
      await expect(page.getByTestId('replay-field-ruleset')).toContainText('1');
      await expect(page.getByTestId('replay-verdict-raw')).toHaveAttribute(
        'href',
        `/api/fights/${ID.ko}`,
      );
    });

    const flip = (hex: string) =>
      hex.endsWith('0') ? `${hex.slice(0, -1)}1` : `${hex.slice(0, -1)}0`;

    for (const [name, alter, check] of [
      ['the log', (d: FightDetail) => ({ ...d, log: `${d.log} ` }), 'log'],
      ['the seed', (d: FightDetail) => ({ ...d, seed: flip(d.seed ?? '') }), 'commit'],
      [
        'the commit',
        (d: FightDetail) => ({
          ...d,
          fight: { ...d.fight, seed_commit: flip(d.fight.seed_commit ?? '') },
        }),
        'commit',
      ],
    ] as const) {
      test(`${name} altered: it does not match, and the fight still shows`, async ({ page }) => {
        await answered(page, ID.ko, alter(ko));
        await page.goto(`/es/fights/${ID.ko}`);
        await expect(verdict(page)).toHaveAttribute('data-status', 'mismatch', { timeout: 30_000 });
        await expect(page.getByTestId('replay-verdict-check')).toHaveAttribute('data-check', check);
        await expect(page.getByTestId('replay-verdict-text')).toHaveText(
          ES.replay_verify_mismatch ?? '',
        );
        await expect(page.getByTestId('replay-verdict-limit')).toHaveCount(0);
        await expect(result(page)).toHaveCount(0);
      });
    }

    for (const [name, alter, check] of [
      ['final: null', (d: FightDetail) => ({ ...d, fight: { ...d.fight, final: null } }), 'state'],
      [
        'state: "live"',
        (d: FightDetail) => ({ ...d, fight: { ...d.fight, state: 'live' } }),
        'state',
      ],
      ['another id', (d: FightDetail) => ({ ...d, fight: { ...d.fight, id: 99 } }), 'data'],
      ['no shape at all', () => ({ fight: { id: ID.ko } }), 'data'],
    ] as const) {
      test(`${name}: only the mismatch, and no field of it`, async ({ page }) => {
        await answered(page, ID.ko, alter(ko));
        await page.goto(`/es/fights/${ID.ko}`);
        await expect(verdict(page)).toHaveAttribute('data-status', 'mismatch', { timeout: 30_000 });
        await expect(page.getByTestId('replay-verdict-check')).toHaveAttribute('data-check', check);
        await expect(stage(page)).toHaveCount(0);
        await expect(page.getByTestId('replay-controls')).toHaveCount(0);
        await expect(page.getByTestId('replay-show-result')).toHaveCount(0);
        await expect(result(page)).toHaveCount(0);
        await page.getByTestId('replay-verdict-more').locator('summary').click();
        await expect(page.getByTestId('replay-field-seed')).toHaveCount(0);
        await expect(page.getByTestId('replay-field-commit')).toHaveCount(0);
        await expect(page.getByTestId('replay-verdict-more')).not.toContainText(ko.seed ?? '-');
        // Who fights still comes from the SSR.
        await expect(page.getByRole('heading', { level: 1 })).toContainText('Luchador striker');
      });
    }

    test('an unknown ruleset, with its hash: could not check, and a reload', async ({ page }) => {
      const a = ko.spec_a ?? '';
      const b = ko.spec_b ?? '';
      await answered(page, ID.ko, {
        ...ko,
        fight: {
          ...ko.fight,
          ruleset_version: 3,
          snapshot_sha256: sha256(`{"ruleset_version":3,"a":${a},"b":${b}}`),
        },
      });
      await page.goto(`/en/fights/${ID.ko}`);
      await expect(verdict(page)).toHaveAttribute('data-status', 'cannot_check', {
        timeout: 30_000,
      });
      await expect(page.getByTestId('replay-verdict-text')).toContainText('version 3');
      await expect(page.getByTestId('replay-verdict-reload')).toBeVisible();
    });

    test('a read that fails is a retry, and no verdict', async ({ page }) => {
      let fail = true;
      await page.route(`**/api/fights/${ID.ko}`, (route) =>
        fail ? route.fulfill({ status: 503, json: {} }) : route.fulfill({ json: ko }),
      );
      await page.goto(`/es/fights/${ID.ko}`);
      await waitForHydration(page);
      await expect(page.getByTestId('replay-unavailable')).toBeVisible();
      await expect(verdict(page)).toHaveCount(0);
      fail = false;
      await page.getByTestId('replay-load-retry').click();
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
    });

    test('a 200 that is not JSON is a retry too', async ({ page }) => {
      await page.route(`**/api/fights/${ID.ko}`, (route) =>
        route.fulfill({ contentType: 'text/html', body: '<html>' }),
      );
      await page.goto(`/es/fights/${ID.ko}`);
      await expect(page.getByTestId('replay-unavailable')).toBeVisible();
      await expect(verdict(page)).toHaveCount(0);
    });
  });

  test.describe('the fights without a log (RF22)', () => {
    for (const [name, id, reason, text, status] of [
      ['voided', ID.voided, 'engine_error', 'replay_final_engine_error', 'match'],
      ['missed', ID.missed, 'missed_window', 'replay_final_missed_window', 'match'],
      [
        'unrunnable',
        ID.unrunnable,
        'fight_unrunnable',
        'replay_final_fight_unrunnable',
        'not_applicable',
      ],
    ] as const) {
      test(`${name}: its motive and its check, and no player`, async ({ page }) => {
        await page.goto(`/es/fights/${id}`);
        await expect(page.getByTestId('replay-final')).toHaveAttribute('data-reason', reason);
        await expect(page.getByTestId('replay-final')).toHaveText(ES[text] ?? '');
        await expect(verdict(page)).toHaveAttribute('data-status', status, { timeout: 30_000 });
        await expect(stage(page)).toHaveCount(0);
        await expect(page.getByTestId('replay-controls')).toHaveCount(0);
        await expect(page.getByTestId('replay-show-result')).toHaveCount(0);
      });
    }

    test('voided or cancelled never download the wasm', async ({ page }) => {
      const asked: string[] = [];
      page.on('request', (request) => asked.push(request.url()));
      await page.goto(`/es/fights/${ID.missed}`);
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
      await page.goto(`/es/fights/${ID.voided}`);
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
      expect(asked.filter((url) => url.endsWith('.wasm'))).toEqual([]);
    });

    test('a cancelled fight has no sentence about seeds, nor their fields', async ({ page }) => {
      await page.goto(`/es/fights/${ID.missed}`);
      await page.getByTestId('replay-verdict-more').locator('summary').click();
      await expect(page.getByTestId('replay-field-seed')).toHaveCount(0);
      await expect(page.getByTestId('replay-field-commit')).toHaveCount(0);
      await expect(page.getByTestId('replay-field-snapshot')).toBeVisible();
      await expect(page.getByTestId('replay-verdict-more')).not.toContainText(
        'probar varias semillas',
      );
    });

    test('a reason this build does not know: "voided", and could not check', async ({ page }) => {
      const response = await page.request.get(`/api/fights/${ID.voided}`);
      const voided = (await response.json()) as FightDetail;
      await answered(page, ID.voided, {
        ...voided,
        seed: null,
        fight: {
          ...voided.fight,
          final: { kind: 'voided', reason: 'meteor' } as unknown as FightFinal,
        },
      });
      await page.goto(`/es/fights/${ID.voided}`);
      await expect(page.getByTestId('replay-final')).toHaveText(ES.replay_final_voided ?? '');
      await expect(verdict(page)).toHaveAttribute('data-status', 'cannot_check', {
        timeout: 30_000,
      });
      await expect(page.getByTestId('replay-verdict-reload')).toBeVisible();
    });

    // 012 RF22: a lost seed says it cannot be verified, with neither the check nor the seed.
    test('a lost seed: its own notice, and no check unless the data does not hold', async ({
      page,
    }) => {
      const response = await page.request.get(`/api/fights/${ID.voided}`);
      const voided = (await response.json()) as FightDetail;
      const lost = { ...voided.fight, final: { kind: 'voided', reason: 'secret_lost' } as const };
      await answered(page, ID.voided, { ...voided, seed: null, fight: lost });
      await page.goto(`/es/fights/${ID.voided}`);
      await expect(page.getByTestId('replay-final')).toHaveAttribute('data-reason', 'secret_lost');
      await expect(page.getByTestId('replay-final')).toHaveText(ES.replay_final_secret_lost ?? '');
      await waitForHydration(page);
      await expect(verdict(page)).toHaveCount(0);
      await expect(page.getByTestId('replay-field-seed')).toHaveCount(0);

      // The same final with its seed still published is not a lost seed.
      await answered(page, ID.voided, { ...voided, fight: lost });
      await page.reload();
      await expect(verdict(page)).toHaveAttribute('data-status', 'mismatch', { timeout: 30_000 });
      await expect(page.getByTestId('replay-verdict-check')).toHaveAttribute(
        'data-check',
        'commit',
      );
    });
  });

  test.describe('a fight that has not ended (RF23)', () => {
    test('"not yet", with a link to the live page, and it downloads nothing', async ({ page }) => {
      const asked: string[] = [];
      page.on('request', (request) => asked.push(new URL(request.url()).pathname));
      const response = await page.goto(`/es/fights/${ID.scheduled}`);
      expect(response?.status()).toBe(200);
      await waitForHydration(page);
      await expect(page.getByTestId('fight-pending')).toContainText(ES.replay_pending_text ?? '');
      await expect(page.getByTestId('fight-pending').getByRole('link')).toHaveAttribute(
        'href',
        '/es/',
      );
      await expect(stage(page)).toHaveCount(0);
      await expect(verdict(page)).toHaveCount(0);
      expect(asked.filter((file) => file.startsWith('/api/fights'))).toEqual([]);
      expect(asked.filter((file) => file.endsWith('.wasm'))).toEqual([]);
    });
  });

  test.describe('what it looks like', () => {
    const STATES: {
      name: string;
      url: (locale: string) => string;
      ready: (page: Page) => Promise<unknown>;
      body?: () => unknown;
    }[] = [
      {
        name: 'list',
        url: (locale) => `/${locale}/fights`,
        ready: (page) => expect(page.getByTestId('fight-list-row').first()).toBeVisible(),
      },
      {
        name: 'paused',
        url: (locale) => `/${locale}/fights/${ID.ko}`,
        ready: async (page) => {
          await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
          await stageUp(page);
          await page.waitForTimeout(500);
        },
      },
      {
        name: 'finished',
        url: (locale) => `/${locale}/fights/${ID.decision}`,
        ready: async (page) => {
          await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
          await stageUp(page);
          await page.getByTestId('replay-end').click();
          await expect(result(page)).toBeVisible();
          await page.getByTestId('replay-verdict-more').locator('summary').click();
          await page.waitForTimeout(500);
        },
      },
      {
        name: 'mismatch',
        url: (locale) => `/${locale}/fights/${ID.ko}`,
        body: () => ({ ...ko, log: `${ko.log} ` }),
        ready: async (page) => {
          await expect(verdict(page)).toHaveAttribute('data-status', 'mismatch', {
            timeout: 30_000,
          });
          await stageUp(page);
          await page.waitForTimeout(500);
        },
      },
      {
        name: 'untrusted',
        url: (locale) => `/${locale}/fights/${ID.ko}`,
        body: () => ({ ...ko, fight: { ...ko.fight, id: 99 } }),
        ready: (page) =>
          expect(verdict(page)).toHaveAttribute('data-status', 'mismatch', { timeout: 30_000 }),
      },
      {
        name: 'cancelled',
        url: (locale) => `/${locale}/fights/${ID.missed}`,
        ready: (page) =>
          expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 }),
      },
      {
        name: 'pending',
        url: (locale) => `/${locale}/fights/${ID.scheduled}`,
        ready: (page) => expect(page.getByTestId('fight-pending')).toBeVisible(),
      },
    ];

    // Every state in one combination each way, and all of them in the states that have the most.
    const WHOLE = new Set(['list', 'paused', 'finished', 'cancelled']);
    const SOME = new Set(['es-mobile-light', 'en-desktop-dark']);

    for (const state of STATES) {
      for (const theme of ['light', 'dark'] as const) {
        for (const locale of ['es', 'en'] as const) {
          for (const size of SIZES) {
            if (!WHOLE.has(state.name) && !SOME.has(`${locale}-${size.name}-${theme}`)) continue;
            test(`screenshot ${state.name} ${locale} ${size.name} ${theme}`, async ({
              page,
            }, info) => {
              test.skip(info.project.name !== 'chromium', 'the artifacts are of one engine');
              await page
                .context()
                .addCookies([{ name: 'theme', value: theme, url: PUBLIC_ORIGIN }]);
              await page.setViewportSize(size);
              const body = state.body?.();
              if (body !== undefined) await answered(page, ID.ko, body);
              // FIGHT_READ_IP gives 2 reads per second, and a page takes two (preocupacion 11).
              await page.waitForTimeout(1000);
              await page.goto(state.url(locale));
              await waitForHydration(page);
              await state.ready(page);
              expect(await overflow(page)).toBeLessThanOrEqual(0);
              await page.screenshot({
                path: `${SHOTS}/replay-${state.name}-${locale}-${size.name}-${theme}.png`,
                fullPage: true,
              });
            });
          }
        }
      }
    }

    test('screenshot at the end, on a phone with WebKit', async ({ page }, info) => {
      test.skip(info.project.name !== 'webkit-mobile', 'the other engine has its own');
      await page.goto(`/es/fights/${ID.ko}`);
      await waitForHydration(page);
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
      if (await stageUp(page)) {
        await page.getByTestId('replay-end').click();
        await expect(result(page)).toBeVisible();
      }
      await page.waitForTimeout(500);
      await page.screenshot({ path: `${SHOTS}/replay-webkit-finished-375.png`, fullPage: true });
    });

    test('axe with the check open and the result shown', async ({ page }) => {
      await page.goto(`/es/fights/${ID.ko}`);
      await expect(verdict(page)).toHaveAttribute('data-status', 'match', { timeout: 30_000 });
      await stageUp(page);
      await page.getByTestId('replay-show-result').click();
      await page.getByTestId('replay-verdict-more').locator('summary').click();
      // The controls come from `disabled`, and their opacity has a transition.
      await page.waitForTimeout(500);
      const results = await new AxeBuilder({ page })
        .withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa'])
        .analyze();
      const serious = results.violations.filter(
        (v) => v.impact === 'serious' || v.impact === 'critical',
      );
      expect(serious.map((v) => v.id)).toEqual([]);
    });
  });
});
