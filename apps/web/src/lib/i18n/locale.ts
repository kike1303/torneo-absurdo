import { baseLocale, locales, type Locale } from '#lib/paraglide/runtime.js';

export type { Locale };

/** Name of the language preference cookie (RF-A4). Written only by the locale switcher. */
export const LOCALE_COOKIE = 'locale';
/** One year, like the `theme` cookie (RF-A4, ADR-0009 D3). */
export const LOCALE_COOKIE_MAX_AGE = 31_536_000;

/** Paths outside locale routing (RF-A3). Matched by whole segment: `/api` and `/api/x`, not `/apix`. */
const EXCLUDED_SEGMENTS = ['healthz', 'api', 'ws'] as const;

export function isLocale(value: unknown): value is Locale {
  return typeof value === 'string' && (locales as readonly string[]).includes(value);
}

export function isExcludedPath(pathname: string): boolean {
  return EXCLUDED_SEGMENTS.some(
    (segment) => pathname === `/${segment}` || pathname.startsWith(`/${segment}/`),
  );
}

/** The locale prefix of a pathname (`/es`, `/es/…`), or `undefined` if it has none. */
export function localeFromPathname(pathname: string): Locale | undefined {
  const match = /^\/([^/]+)(?:\/|$)/.exec(pathname);
  const candidate = match?.[1];
  return isLocale(candidate) ? candidate : undefined;
}

/** Removes every leading `/` and `\` so the result can never start a protocol-relative URL. */
export function stripLeadingSlashes(path: string): string {
  return path.replace(/^[/\\]+/, '');
}

/**
 * Parses `Accept-Language` and returns the supported locale with the highest weight.
 * Region subtags fall back to their language (`es-MX` → `es`). Entries with `q=0` are ignored.
 */
export function localeFromAcceptLanguage(header: string | null | undefined): Locale | undefined {
  if (!header) return undefined;
  let best: { locale: Locale; q: number; index: number } | undefined;
  header.split(',').forEach((part, index) => {
    const [rawTag = '', ...params] = part.trim().split(';');
    const tag = rawTag.trim().toLowerCase();
    if (!tag) return;
    let q = 1;
    for (const param of params) {
      const [key, value] = param.trim().split('=');
      if (key?.trim() === 'q') {
        const parsed = Number(value);
        q = Number.isFinite(parsed) ? parsed : 0;
      }
    }
    if (q <= 0) return;
    const language = tag.split('-')[0];
    if (!isLocale(language)) return;
    if (!best || q > best.q || (q === best.q && index < best.index)) {
      best = { locale: language, q, index };
    }
  });
  return best?.locale;
}

/**
 * Locale for a path without prefix (RF-A3, 008 RF-D4): the profile's preference (with a session)
 * → valid `locale` cookie → `Accept-Language` → `en`.
 */
export function pickLocale(
  profile: string | null | undefined,
  cookie: string | null | undefined,
  acceptLanguage: string | null | undefined,
): Locale {
  if (isLocale(profile)) return profile;
  if (isLocale(cookie)) return cookie;
  return localeFromAcceptLanguage(acceptLanguage) ?? baseLocale;
}

/**
 * Same path and query in another locale, always as a relative path that starts with `/<locale>/`
 * (RF-A5). Accepts a pathname with or without a locale prefix, plus an optional search string.
 */
export function localizedHref(pathname: string, search: string, locale: Locale): string {
  const prefix = localeFromPathname(pathname);
  const rest = prefix ? pathname.slice(prefix.length + 1) : pathname;
  const query = search === '' || search.startsWith('?') ? search : `?${search}`;
  return `/${locale}/${stripLeadingSlashes(rest)}${query}`;
}

/** Serialized `locale` cookie (RF-A4). `Secure` only on https. Never `HttpOnly`: the client writes it. */
export function serializeLocaleCookie(locale: Locale, protocol: string): string {
  const secure = protocol === 'https:' ? '; Secure' : '';
  return `${LOCALE_COOKIE}=${locale}; Path=/; Max-Age=${LOCALE_COOKIE_MAX_AGE}; SameSite=Lax${secure}`;
}

/** Writes the `locale` cookie. Only called when the user picks a language in the switcher. */
export function writeLocaleCookie(locale: Locale): void {
  if (!isLocale(locale)) return;
  document.cookie = serializeLocaleCookie(locale, location.protocol);
}
