import type { Handle } from '@sveltejs/kit/hooks';
import { loginRedirectSearch } from '#lib/features/auth/login-url.js';
import {
  LOCALE_COOKIE,
  isExcludedPath,
  localeFromPathname,
  pickLocale,
  stripLeadingSlashes,
  type Locale,
} from '#lib/i18n/locale.js';

/**
 * `Location` for a path without locale prefix. Built only from the WHATWG-serialized `pathname`
 * and `search`, so `//evil.example/` or `/\evil.example` can never become a protocol-relative
 * redirect, and `%0d%0a` stays percent-encoded (RF-A3, plan-a decision 3). `/login` carries
 * only a listed `error` (`loginRedirectSearch`), never any other query.
 */
export function localeLocation(locale: Locale, pathname: string, search: string): string {
  const path = stripLeadingSlashes(pathname);
  // The server's callback lands on the unprefixed /login: only a listed `error` goes on.
  if (path === 'login' || path === 'login/')
    return `/${locale}/login${loginRedirectSearch(search)}`;
  return `/${locale}/${path}${search}`;
}

/**
 * Redirects every non-excluded path without `/es` or `/en` prefix with a relative 307 (RF-A3).
 * With a session, the profile's language comes first (008 RF-D4).
 */
export const localeRedirect: Handle = ({ event, resolve }) => {
  const { pathname, search } = event.url;
  if (isExcludedPath(pathname) || localeFromPathname(pathname) !== undefined) {
    return resolve(event);
  }
  const locale = pickLocale(
    event.locals.user?.locale ?? null,
    event.cookies.get(LOCALE_COOKIE),
    event.request.headers.get('accept-language'),
  );
  return new Response(null, {
    status: 307,
    headers: {
      Location: localeLocation(locale, pathname, search),
      Vary: 'Cookie, Accept-Language',
      'Cache-Control': 'private, no-store',
    },
  });
};
