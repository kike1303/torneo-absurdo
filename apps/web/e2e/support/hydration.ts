import type { Page } from '@playwright/test';

/**
 * Waits until SvelteKit's client has started on the current document (TA-12).
 *
 * The SSR HTML is already complete (including `data-theme` and `lang`), so a visible element says
 * nothing about hydration, and the client start can land after the `load` event. Interacting
 * before it races the app: a click may hit a control with no handlers yet, and the start's
 * `history.replaceState(…, '')` shows up as a same-URL `framenavigated`. That start stamps
 * `sveltekit:metadata` into `history.state`, which is what this waits for. If SvelteKit renames
 * the key, this times out loudly instead of passing silently.
 */
export async function waitForHydration(page: Page): Promise<void> {
  await page.waitForFunction(() => {
    const state = history.state as Record<string, unknown> | null;
    return state !== null && 'sveltekit:metadata' in state;
  });
}
