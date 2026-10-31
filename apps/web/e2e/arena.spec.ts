import AxeBuilder from '@axe-core/playwright';
import { expect, test, type Page } from '@playwright/test';
import spike from '@torneo/renderer/fixtures/spike.timeline.json' with { type: 'json' };
import { validateTimeline } from '@torneo/renderer/timeline';
import comeback from '../src/lib/features/fight/fixtures/log-ko-comeback.json' with { type: 'json' };
import clutch from '../src/lib/features/fight/fixtures/log-ko-clutch.json' with { type: 'json' };
import decisiveCrit from '../src/lib/features/fight/fixtures/log-ko-decisive-crit.json' with { type: 'json' };
import knockdownLog from '../src/lib/features/fight/fixtures/log-ko-knockdown.json' with { type: 'json' };
import decisionA from '../src/lib/features/fight/fixtures/log-decision-a.json' with { type: 'json' };
import decisionB from '../src/lib/features/fight/fixtures/log-decision-b.json' with { type: 'json' };
import draw from '../src/lib/features/fight/fixtures/log-draw.json' with { type: 'json' };
import { paceMin } from '../src/lib/features/arena/retime.js';
import {
  timelineFromFightLog,
  type AdapterLog,
} from '../src/lib/features/fight/timeline-from-fight-log.js';
import { waitForHydration } from './support/hydration.js';

// 002 RF-A15, RF-A16 and RF-A19 (plan-a2 step 7): the test arena on the production build, with
// WebGL by software in Chromium. Screenshots are artifacts for review, not pixel comparisons.
const ORIGIN = 'http://localhost:4173';
const DIR = 'test-results/screenshots';
const DURATION = spike.duration_ms;
const stage = (page: Page) => page.getByTestId('fight-stage');
const F1_LIMIT_MS = 180_000;

/** TA-20: the arena loads paused (`ready`); playback only starts with Play. */
async function play(page: Page): Promise<void> {
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await waitForHydration(page);
  await page.getByTestId('arena-toggle').click();
  await expect(stage(page)).toHaveAttribute('data-state', 'playing');
}

/** It stays paused: same state and same last cue after `ms`. */
async function staysPaused(page: Page, ms: number): Promise<void> {
  const cue = await stage(page).getAttribute('data-last-cue');
  await page.waitForTimeout(ms);
  await expect(stage(page)).toHaveAttribute('data-state', 'ready');
  expect(await stage(page).getAttribute('data-last-cue')).toBe(cue);
}

/** Resolves after `count` frames of the page, which take longer the busier the machine is. */
async function animationFrames(page: Page, count: number): Promise<void> {
  await page.evaluate(async (frames) => {
    for (let i = 0; i < frames; i++) await new Promise(requestAnimationFrame);
  }, count);
}

/**
 * Runs `body` with the trace of the context paused: on this canvas the trace multiplies the time by
 * four (TA-32), and `trace` is a worker option, so `test.use` cannot turn it off for one test. The
 * chunk comes back afterwards because the runner stops it when the context closes.
 */
async function withoutTracing(page: Page, body: () => Promise<void>): Promise<void> {
  const { tracing } = page.context();
  // It rejects when this run records no trace, whatever the trace mode says.
  const traced = await tracing.stopChunk().then(
    () => true,
    () => false,
  );
  try {
    await body();
  } finally {
    if (traced) await tracing.startChunk();
  }
}

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
  test(`/${lang}/dev/arena loads paused, plays with Play, without console errors`, async ({
    page,
  }) => {
    const errors = trackErrors(page);
    await page.goto(`/${lang}/dev/arena`);
    await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
    await expect(
      page.getByRole('img', { name: lang === 'es' ? /Muñeco A/ : /Dummy A/ }),
    ).toBeVisible();
    await expect(page.getByTestId('arena-toggle')).toHaveText(
      lang === 'es' ? 'Reproducir' : 'Play',
    );
    await staysPaused(page, 1000);
    await play(page);
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
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  const before = [
    await stage(page).getAttribute('data-hp-a'),
    await stage(page).getAttribute('data-hp-b'),
  ];
  await play(page);
  await expect
    .poll(async () => [
      await stage(page).getAttribute('data-hp-a'),
      await stage(page).getAttribute('data-hp-b'),
    ])
    .not.toEqual(before);
  await page.getByTestId('arena-toggle').click();
  await expect(stage(page)).toHaveAttribute('data-state', 'paused');
  await page.getByTestId('arena-toggle').click();
  await expect(stage(page)).toHaveAttribute('data-state', 'playing');
});

test('RF-A19: after a seek to the middle and 3 s of playback, frames > 0', async ({ page }) => {
  await page.goto(`/es/dev/arena?t=${Math.round(DURATION / 2)}`);
  await play(page);
  await page.getByTestId('arena-show-stats').check();
  await page.waitForTimeout(3000);
  await expect
    .poll(async () =>
      Number((await page.getByTestId('arena-stats-frames').textContent())?.replace(/\D/g, '')),
    )
    .toBeGreaterThan(0);
});

test('changing a fighter reloads the arena and leaves it paused (TA-20)', async ({ page }) => {
  await page.goto('/en/dev/arena');
  await play(page);
  await page.getByTestId('arena-fighter-b').selectOption('dummy_a');
  await expect(
    page.getByRole('img', { name: 'Animated fight between Dummy A and Dummy A' }),
  ).toBeAttached();
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await staysPaused(page, 1000);
  await play(page);
});

test('the instance without the catalog answers 404', async ({ request }) => {
  const res = await request.get('http://localhost:4174/es/dev/arena');
  expect(res.status()).toBe(404);
});

for (const theme of ['light', 'dark'] as const) {
  test(`axe without serious or critical violations (theme=${theme})`, async ({ page }) => {
    await page.context().addCookies([{ name: 'theme', value: theme, url: ORIGIN }]);
    await page.goto('/es/dev/arena');
    await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
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
        await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
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

// QA findings of A2.
test('F1: switching a fighter quickly many times leaves one paused arena, without errors', async ({
  browser,
}) => {
  // Each switch builds a renderer with WebGL by software, so the time goes to real work that grows
  // with the load of the machine, not to the waits: it gets its own limit (TA-32).
  test.setTimeout(F1_LIMIT_MS);
  // The race is timing dependent, so it runs on several fresh pages (the unit test of
  // shared-cache.ts is the deterministic one).
  for (let run = 0; run < 4; run++) {
    const page = await browser.newPage();
    await withoutTracing(page, async () => {
      const errors = trackErrors(page);
      await page.goto('/es/dev/arena?t=1500');
      // Not `ready`: the race needs a superseded renderer still loading a sheet, and once the first
      // load is over the sheets come from the cache (TA-32: with the race back, 0 of 16 pages
      // failed when the burst waited for `ready`).
      await waitForHydration(page);
      const select = page.getByTestId('arena-fighter-b');
      for (let i = 0; i < 6; i++) {
        // `force`: the select is disabled while loading, which is when the race can happen.
        await select.selectOption(i % 2 ? 'dummy_b' : 'dummy_a', { force: true });
        // Not a settling wait: the growing gaps make the switches land on different load stages.
        await page.waitForTimeout(i * 40);
      }
      await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
      // A broken texture throws when it is drawn: the frames, however long they take, have to pass.
      await animationFrames(page, 10);
      await expect(stage(page)).toHaveAttribute('data-state', 'ready');
      expect(errors, `run ${run}`).toEqual([]);
      await expect(page.locator('[data-testid="fight-stage"] canvas')).toHaveCount(1);
    });
    await page.close();
  }
});

test('F3: "Copied" stays while the stats refresh', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  await page.goto('/es/dev/arena');
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await waitForHydration(page);
  await page.getByTestId('arena-show-stats').click();
  await page.getByTestId('arena-copy-stats').click();
  await expect(page.getByText('Copiado')).toBeVisible();
  // The panel refreshes every 500 ms; the message outlives several refreshes.
  await page.waitForTimeout(1200);
  await expect(page.getByText('Copiado')).toBeVisible();
  const copied = JSON.parse(await page.evaluate(() => navigator.clipboard.readText()));
  expect(copied).toMatchObject({ fighters: ['dummy_a', 'dummy_b'] });
});

// TA-20: on the LAN URL of `vite dev` (an insecure context) a phone has no Clipboard API. The JSON
// is shown selected for a manual copy, with a message, whether the API is missing or rejects.
const NO_CLIPBOARD = {
  undefined: () => {
    Object.defineProperty(Navigator.prototype, 'clipboard', { get: () => undefined });
  },
  rejecting: () => {
    Object.defineProperty(Navigator.prototype, 'clipboard', {
      get: () => ({ writeText: () => Promise.reject(new DOMException('no', 'NotAllowedError')) }),
    });
  },
} as const;
for (const [name, script] of Object.entries(NO_CLIPBOARD)) {
  test(`TA-20: with the clipboard ${name}, "Copy JSON" shows the JSON to copy by hand`, async ({
    page,
  }) => {
    await page.addInitScript(script);
    await page.goto('/es/dev/arena');
    await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
    await waitForHydration(page);
    await page.getByTestId('arena-show-stats').click();
    await expect(page.getByTestId('arena-copy-manual')).toHaveCount(0);
    await page.getByTestId('arena-copy-stats').click();
    const manual = page.getByTestId('arena-copy-manual');
    await expect(manual).toBeVisible();
    await expect(manual).toBeFocused();
    await expect(manual).toHaveAttribute('readonly', '');
    await expect(page.getByText('No se pudo copiar. Copia el JSON a mano.')).toBeVisible();
    await expect(page.getByText('Copiado')).toHaveCount(0);
    expect(JSON.parse(await manual.inputValue())).toMatchObject({
      fighters: ['dummy_a', 'dummy_b'],
    });
    const selected = await manual.evaluate(
      (el) => (el as HTMLTextAreaElement).selectionEnd - (el as HTMLTextAreaElement).selectionStart,
    );
    expect(selected).toBe((await manual.inputValue()).length);
  });
}

test('F4: ?t= with many digits is clamped to the end', async ({ page }) => {
  await page.goto('/es/dev/arena?t=99999999999');
  await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
});

// 002 RF-B7 (phase B): the serving of content/_local/sprites/ only exists in `vite dev`.
test('the build has no local sprites: 404 and only the placeholders in the selector', async ({
  page,
  request,
}) => {
  for (const url of ['/__local-sprites/index.json', '/__local-sprites/dummy_a/manifest.json']) {
    expect((await request.get(url)).status(), url).toBe(404);
  }
  const requested: string[] = [];
  page.on('request', (req) => requested.push(new URL(req.url()).pathname));
  await page.goto('/es/dev/arena');
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await waitForHydration(page);
  for (const side of ['a', 'b']) {
    const values = await page
      .getByTestId(`arena-fighter-${side}`)
      .locator('option')
      .evaluateAll((options) => options.map((o) => (o as HTMLOptionElement).value));
    expect(values).toEqual(['dummy_a', 'dummy_b']);
  }
  expect(requested.filter((p) => p.startsWith('/__local-sprites'))).toEqual([]);
});

// 002 RF-B9: ?pace=<factor> rescales the script; it still validates and plays to the end.
test('?pace=1.25 plays and, at the end, finished', async ({ page }) => {
  const errors = trackErrors(page);
  await page.goto('/es/dev/arena?pace=1.25');
  await play(page);
  await page.goto('/es/dev/arena?pace=1.25&t=99999999999');
  await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
  await expect(page.getByTestId('fight-winner')).toHaveText('Gana Muñeco A');
  expect(errors).toEqual([]);
});

test('?pace=1.5 scales mean_strike_gap_ms in the copied stats', async ({ page, context }) => {
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const strikes = spike.cues.filter((c) => c.type === 'strike').map((c) => c.at_ms);
  const base = ((strikes.at(-1) ?? 0) - (strikes[0] ?? 0)) / (strikes.length - 1);
  await page.goto('/es/dev/arena?pace=1.5');
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await waitForHydration(page);
  await page.getByTestId('arena-show-stats').click();
  await page.getByTestId('arena-copy-stats').click();
  await expect(page.getByText('Copiado')).toBeVisible();
  const copied = JSON.parse(await page.evaluate(() => navigator.clipboard.readText()));
  expect(copied.mean_strike_gap_ms).toBeGreaterThan(base * 1.45);
  expect(copied.mean_strike_gap_ms).toBeLessThan(base * 1.55);
});

// 002 RF-C3 (plan-c step 4): ?log=<name> plays an engine log through the adapter. The winner and
// the HP come from the arena copy, never from literals: the goldens are regenerated (006 decision 9).
function adapted(log: unknown) {
  const result = timelineFromFightLog(log as AdapterLog, {
    a: { asset: 'dummy_a' },
    b: { asset: 'dummy_b' },
  });
  if (!result.ok) throw new Error('the arena copy does not adapt');
  return result.timeline;
}

type Side = 'a' | 'b';

function koWinner(log: { events: readonly { type: string; winner?: string | null }[] }): {
  winner: Side;
  loser: Side;
} {
  const winner = log.events.at(-1)?.winner;
  if (winner === 'a') return { winner, loser: 'b' };
  if (winner === 'b') return { winner, loser: 'a' };
  throw new Error('the log does not end with a winner');
}

for (const lang of ['es', 'en'] as const) {
  const NAMES = {
    a: lang === 'es' ? 'Muñeco A' : 'Dummy A',
    b: lang === 'es' ? 'Muñeco B' : 'Dummy B',
  };
  const wins = (side: Side) => (lang === 'es' ? `Gana ${NAMES[side]}` : `${NAMES[side]} wins`);
  const { winner, loser } = koWinner(comeback);

  test(`/${lang}/dev/arena?log=ko-comeback plays without console errors`, async ({ page }) => {
    const errors = trackErrors(page);
    await page.goto(`/${lang}/dev/arena?log=ko-comeback`);
    await play(page);
    await page.waitForTimeout(1000);
    expect(errors).toEqual([]);
  });

  test(`/${lang}/dev/arena?log=ko-comeback at the end: its winner, the loser at 0 HP`, async ({
    page,
  }) => {
    const errors = trackErrors(page);
    await page.goto(`/${lang}/dev/arena?log=ko-comeback&t=99999999999`);
    await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
    await expect(page.getByTestId('fight-winner')).toHaveText(wins(winner));
    await expect(stage(page)).toHaveAttribute(`data-hp-${loser}`, '0');
    expect(errors).toEqual([]);
  });

  test(`/${lang}/dev/arena?log=<unknown> plays the spike script`, async ({ page }) => {
    await page.goto(`/${lang}/dev/arena?log=desconocido&t=99999999999`);
    await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
    await expect(page.getByTestId('fight-winner')).toHaveText(wins('a'));
  });
}

test('?log=ko-knockdown just after a knockdown: the last cue is that knockdown', async ({
  page,
}) => {
  const timeline = adapted(knockdownLog);
  const index = timeline.cues.findIndex((c) => c.type === 'knockdown');
  const knockdown = timeline.cues[index];
  if (knockdown === undefined) throw new Error('no knockdown in ko-knockdown');
  await page.goto(`/es/dev/arena?log=ko-knockdown&t=${knockdown.at_ms + 200}`);
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await expect(stage(page)).toHaveAttribute('data-last-cue', String(index));
});

// 006 RF16 and RF17: the seven goldens play to the end with any `?pace`. The minimum is computed
// here with the same validator the arena uses, never fixed.
const ENGINE_LOGS = [
  { name: 'ko-comeback', log: comeback, ending: 'ko' },
  { name: 'ko-clutch', log: clutch, ending: 'ko' },
  { name: 'ko-decisive-crit', log: decisiveCrit, ending: 'ko' },
  { name: 'ko-knockdown', log: knockdownLog, ending: 'ko' },
  { name: 'decision-a', log: decisionA, ending: 'decision' },
  { name: 'decision-b', log: decisionB, ending: 'decision' },
  { name: 'draw', log: draw, ending: 'draw' },
] as const;
const isValid = (t: Parameters<typeof validateTimeline>[0]) => validateTimeline(t).ok;
const decimal = new Intl.NumberFormat('es', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

for (const { name, log, ending } of ENGINE_LOGS) {
  const min = paceMin(adapted(log), isValid);

  test(`?log=${name}&t=<end> reaches finished with ending ${ending}`, async ({ page }) => {
    const errors = trackErrors(page);
    await page.goto(`/es/dev/arena?log=${name}&t=99999999999`);
    await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
    await expect(stage(page)).toHaveAttribute('data-ending', ending);
    expect(errors).toEqual([]);
  });

  test(`?log=${name}&pace=0 is clamped to the minimum and reaches finished`, async ({ page }) => {
    if (min === null) throw new Error(`${name} is invalid with factor 1`);
    const errors = trackErrors(page);
    await page.goto(`/es/dev/arena?log=${name}&pace=0&t=99999999999`);
    await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
    await expect(stage(page)).toHaveAttribute('data-ending', ending);
    await waitForHydration(page);
    await page.getByTestId('arena-show-stats').click();
    const row = page.getByTestId('arena-stats-pace');
    await expect(row).toHaveText(decimal.format(min));
    await expect(row).toHaveAttribute('data-clamped', 'true');
    await expect(page.getByTestId('arena-stats-pace-clamped')).toHaveText(
      `ajustado desde ${decimal.format(0)}`,
    );
    expect(errors).toEqual([]);
  });

  test(`?log=${name}&pace=1.5 reaches finished`, async ({ page }) => {
    const errors = trackErrors(page);
    await page.goto(`/es/dev/arena?log=${name}&pace=1.5&t=99999999999`);
    await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
    await expect(stage(page)).toHaveAttribute('data-ending', ending);
    expect(errors).toEqual([]);
  });
}

test('?log=ko-knockdown&pace=0.8: the copied JSON has the requested, effective and min pace', async ({
  page,
  context,
}) => {
  const min = paceMin(adapted(knockdownLog), isValid);
  if (min === null) throw new Error('ko-knockdown is invalid with factor 1');
  await context.grantPermissions(['clipboard-read', 'clipboard-write']);
  const errors = trackErrors(page);
  await page.goto('/es/dev/arena?log=ko-knockdown&pace=0.8');
  await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
  await waitForHydration(page);
  await page.getByTestId('arena-show-stats').click();
  await expect(page.getByTestId('arena-stats-pace')).toHaveAttribute(
    'data-clamped',
    String(min > 0.8),
  );
  await page.getByTestId('arena-copy-stats').click();
  await expect(page.getByText('Copiado')).toBeVisible();
  const copied = JSON.parse(await page.evaluate(() => navigator.clipboard.readText()));
  expect(copied).toMatchObject({
    pace_requested: 0.8,
    pace_effective: Math.max(0.8, min),
    pace_min: min,
  });
  expect(errors).toEqual([]);
});

// 005 RF18: the engine logs that end by time. `time_up` at 86 000, the closing (`win` or `draw`) at
// 87 500 and the end at 90 000.
const TIME_LOGS = [
  { name: 'decision-a', log: decisionA, ending: 'decision', winner: 'a' },
  { name: 'decision-b', log: decisionB, ending: 'decision', winner: 'b' },
  { name: 'draw', log: draw, ending: 'draw', winner: null },
] as const;

function timeUpIndex(log: unknown): number {
  const index = adapted(log).cues.findIndex((c) => c.type === 'time_up');
  if (index < 0) throw new Error('no time_up');
  return index;
}

for (const lang of ['es', 'en'] as const) {
  const NAMES = {
    a: lang === 'es' ? 'Muñeco A' : 'Dummy A',
    b: lang === 'es' ? 'Muñeco B' : 'Dummy B',
  };
  const byDecision = (name: string) =>
    lang === 'es' ? `Gana ${name} por decisión` : `${name} wins by decision`;

  for (const { name, log, ending, winner } of TIME_LOGS) {
    test(`/${lang}/dev/arena?log=${name} at the end: ${ending}`, async ({ page }) => {
      const errors = trackErrors(page);
      await page.goto(`/${lang}/dev/arena?log=${name}&t=99999999999`);
      await expect(stage(page)).toHaveAttribute('data-state', 'finished', { timeout: 15_000 });
      await expect(stage(page)).toHaveAttribute('data-ending', ending);
      if (winner === null) {
        await expect(page.getByTestId('fight-draw')).toHaveText(lang === 'es' ? 'Empate' : 'Draw');
        await expect(page.getByTestId('fight-winner')).toHaveCount(0);
      } else {
        await expect(page.getByTestId('fight-winner')).toHaveText(byDecision(NAMES[winner]));
        await expect(page.getByTestId('fight-draw')).toHaveCount(0);
      }
      await expect(
        page.getByRole('button', { name: lang === 'es' ? 'Repetir' : 'Replay' }),
      ).toBeVisible();
      expect(errors).toEqual([]);
    });

    test(`/${lang}/dev/arena?log=${name} at 86 500: the last cue is time_up`, async ({ page }) => {
      const errors = trackErrors(page);
      await page.goto(`/${lang}/dev/arena?log=${name}&t=86500`);
      await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
      await expect(stage(page)).toHaveAttribute('data-last-cue', String(timeUpIndex(log)));
      expect(errors).toEqual([]);
    });
  }
}

// 006 RF17: the pace row and its clamp note, without horizontal scroll, for review.
for (const size of SIZES) {
  for (const lang of ['es', 'en'] as const) {
    test(`screenshot of the pace row ${lang} ${size.name}, without horizontal scroll`, async ({
      page,
    }) => {
      await page.setViewportSize(size);
      await page.goto(`/${lang}/dev/arena?log=ko-knockdown&pace=0`);
      await expect(stage(page)).toHaveAttribute('data-state', 'ready', { timeout: 15_000 });
      await waitForHydration(page);
      await page.getByTestId('arena-show-stats').click();
      await expect(page.getByTestId('arena-stats-pace-clamped')).toBeVisible();
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      );
      expect(overflow).toBeLessThanOrEqual(0);
      await page.getByTestId('arena-stats').screenshot({
        path: `${DIR}/arena-pace-${lang}-${size.name}.png`,
      });
    });
  }
}
