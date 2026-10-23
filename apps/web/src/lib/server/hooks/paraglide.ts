import type { Handle } from '@sveltejs/kit/hooks';
import { isExcludedPath } from '#lib/i18n/locale.js';
// The only importer of the Paraglide server module (plan-a decision 1).
import { paraglideMiddleware } from '#lib/paraglide/server.js';
import { relativizeLocation } from './relativize-location.js';

/**
 * Runs Paraglide's middleware (locale context for SSR, `%paraglide.lang%`) on localized routes.
 * Skipped on excluded paths so it never redirects `/healthz`, `/api/**` or `/ws`. Any redirect it
 * produces is rewritten to a relative `Location` (plan-a decisions 2 and 3).
 */
export const paraglide: Handle = async ({ event, resolve }) => {
  if (isExcludedPath(event.url.pathname)) return resolve(event);
  const response = await paraglideMiddleware(event.request, ({ locale }) =>
    resolve(event, {
      transformPageChunk: ({ html }) => html.replace('%paraglide.lang%', locale),
    }),
  );
  return relativizeLocation(response);
};
