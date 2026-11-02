import { readFileSync } from 'node:fs';
import path from 'node:path';
import { expect, test } from '@playwright/test';
import { waitForHydration } from './support/hydration.js';
import { SERVER_LOG, WEB_DIR, fileLines } from './support/logs.js';

// 011 RF5, RF17, RF18, without fights: the general e2e has no roster, so the database has none. The
// fights themselves are in replays.spec.ts and fights.spec.ts.
const readsOfFights = () =>
  fileLines(SERVER_LOG).filter((line) => line.includes('"route":"/api/fights')).length;

/** The chunks that hold Pixi, from what the build recorded. */
function pixiChunks(): string[] {
  const file = path.join(WEB_DIR, '.svelte-kit/output/client/.vite/chunk-modules.json');
  const chunks = JSON.parse(readFileSync(file, 'utf8')) as Record<string, string[]>;
  const pixi = Object.entries(chunks)
    .filter(([, modules]) => modules.some((id) => /node_modules\/.*pixi\.js\//.test(id)))
    .map(([chunk]) => chunk);
  expect(pixi.length, 'the build has chunks with Pixi').toBeGreaterThan(0);
  return pixi;
}

test.describe('the list without fights', () => {
  for (const [locale, text, link] of [
    ['es', 'Todavía no termina ninguna pelea', '/es/'],
    ['en', 'No fight has ended yet', '/en/'],
  ] as const) {
    test(`empty state, ${locale}`, async ({ page }) => {
      const response = await page.goto(`/${locale}/fights`);
      expect(response?.status()).toBe(200);
      expect(response?.headers()['cache-control']).toContain('no-store');
      await expect(page.getByTestId('fight-list-empty')).toContainText(text);
      await expect(page.getByTestId('fight-list-empty').getByRole('link')).toHaveAttribute(
        'href',
        link,
      );
      await expect(page.getByTestId('fight-list-row')).toHaveCount(0);
    });
  }

  test('a before that is not an id gives the first page, not an error', async ({ page }) => {
    const response = await page.goto('/es/fights?before=..%2Fme');
    expect(response?.status()).toBe(200);
    await expect(page.getByTestId('fight-list-empty')).toBeVisible();
  });
});

test.describe('the URL of a fight (RF18)', () => {
  test('what is not an id is a 404 that never reaches the API', async ({ request }) => {
    const before = readsOfFights();
    for (const segment of [
      '..%2Fme',
      '..%2Fauth%2Fgoogle%2Fcallback%3Fcode%3Dx',
      '12%3Fx',
      '01',
      '0',
      '2147483648',
    ]) {
      const response = await request.get(`/es/fights/${segment}`);
      expect(response.status(), segment).toBe(404);
    }
    expect(readsOfFights()).toBe(before);
  });

  test('a fight that does not exist is a real 404, after asking the API', async ({ page }) => {
    const before = readsOfFights();
    const response = await page.goto('/es/fights/999999');
    expect(response?.status()).toBe(404);
    await expect(page.getByTestId('error-page')).toHaveAttribute('data-status', '404');
    // The positive control of the test above: the log does record these reads.
    await expect.poll(readsOfFights).toBeGreaterThan(before);
  });
});

test.describe('what the first pages load (RF5)', () => {
  for (const url of ['/es/', '/es/fights']) {
    test(`${url} asks for neither the wasm nor Pixi`, async ({ page }) => {
      const pixi = pixiChunks();
      const asked: string[] = [];
      page.on('request', (request) => asked.push(new URL(request.url()).pathname));
      await page.goto(url);
      await waitForHydration(page);
      await page.waitForLoadState('networkidle');
      expect(asked.length).toBeGreaterThan(0);
      expect(asked.filter((file) => file.endsWith('.wasm'))).toEqual([]);
      expect(asked.filter((file) => pixi.some((chunk) => file.endsWith(chunk)))).toEqual([]);
    });
  }
});
