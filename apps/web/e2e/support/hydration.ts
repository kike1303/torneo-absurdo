import type { Page } from '@playwright/test';

/**
 * Waits until the app has hydrated on the current document (TA-12).
 *
 * The SSR HTML is already complete (including `data-theme` and `lang`), so a visible element says
 * nothing about hydration, and the client start can land after the `load` event. Interacting
 * before it races the app in two ways: a click may hit a control with no handlers yet, and the
 * start's `history.replaceState(…, '')` shows up as a same-URL `framenavigated`.
 *
 * The root layout sets `html[data-hydrated]` in `onMount`, which runs after hydration, and
 * SvelteKit hydrates after that `replaceState`, so this one signal closes both races. Waiting for
 * `sveltekit:metadata` in `history.state` would not: SvelteKit writes it before hydrating.
 */
export async function waitForHydration(page: Page): Promise<void> {
  await page.locator('html[data-hydrated]').waitFor({ state: 'attached' });
}
