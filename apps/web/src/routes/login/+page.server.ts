import { redirect } from '@sveltejs/kit';
import { isProvider } from '#lib/features/account/providers.js';
import { resolveCallbackError } from '#lib/features/auth/error-text.js';
import { canonicalLoginQuery } from '#lib/features/auth/login-url.js';
import { safeNext } from '#lib/features/auth/next.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { getProviders } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

/**
 * `/login` (008 RF-D5, RF-D6, plan-d decision 7). The query is canonicalized first: anything that
 * is not a known `error` or a safe `next` gets a 303 to a URL built only from fixed names and
 * validated values, so the page (and the locale switcher, which copies `search`) never sees an
 * unknown value. The page receives a message key, never the value.
 */
export const load: PageServerLoad = async ({ url, locals, fetch }) => {
  const locale = localeFromPathname(url.pathname) ?? 'en';
  const canonical = canonicalLoginQuery(url.search);
  if (canonical.changed) redirect(303, `/${locale}/login${canonical.search}`);
  const params = new URLSearchParams(canonical.search);
  const destination = safeNext(params.get('next'), locale);
  const providers = await getProviders(internalApi(fetch));
  return {
    errorKey: resolveCallbackError(params.get('error')),
    signedIn: locals.user !== null,
    providers:
      providers.ok && Array.isArray(providers.data) ? providers.data.filter(isProvider) : null,
    // Every login goes through /welcome (plan-d open question 4).
    next: `/${locale}/welcome?next=${encodeURIComponent(destination)}`,
  };
};
