import type { Handle } from '@sveltejs/kit/hooks';
import { LOCALE_COOKIE, isExcludedPath, pickLocale } from '#lib/i18n/locale.js';
// The only importer of the Paraglide server module (plan-a decision 1).
import { paraglideMiddleware } from '#lib/paraglide/server.js';
import { relativizeLocation } from './relativize-location.js';

/**
 * Runs Paraglide's middleware (locale context for SSR and `%paraglide.lang%`) on every request.
 *
 * Excluded paths (`/healthz`, `/api/**`, `/ws`) have no locale prefix, so the locale comes from a
 * valid `locale` cookie or falls back to `en`, and the middleware sees a URL that already carries
 * that prefix: it can never redirect them, and any page Kit renders there (a 404) gets a valid
 * `<html lang>` that matches its messages (QA H2). Any redirect is rewritten to a relative
 * `Location` (plan-a decisions 2 and 3).
 */
export const paraglide: Handle = async ({ event, resolve }) => {
  let effectiveRequestUrl: URL | undefined;
  if (isExcludedPath(event.url.pathname)) {
    const locale = pickLocale(null, event.cookies.get(LOCALE_COOKIE), null);
    // The copy only feeds Paraglide's locale detection; its origin (from the client's `Host`)
    // never reaches a response, because `relativizeLocation` rewrites any `Location` below.
    // eslint-disable-next-line no-restricted-syntax -- see above (TA-10, plan-a decision 20)
    effectiveRequestUrl = new URL(event.url);
    effectiveRequestUrl.pathname = `/${locale}${event.url.pathname}`;
  }
  const response = await paraglideMiddleware(
    event.request,
    ({ locale }) =>
      resolve(event, {
        transformPageChunk: ({ html }) => html.replace('%paraglide.lang%', locale),
      }),
    effectiveRequestUrl ? { effectiveRequestUrl } : undefined,
  );
  return relativizeLocation(response);
};
