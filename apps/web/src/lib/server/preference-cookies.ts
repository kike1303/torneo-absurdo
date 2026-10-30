import type { Cookies } from '@sveltejs/kit';
import { LOCALE_COOKIE, LOCALE_COOKIE_MAX_AGE, type Locale } from '#lib/i18n/locale.js';
import { THEME_COOKIE, THEME_COOKIE_MAX_AGE, type ThemeChoice } from '#lib/theme/resolve-theme.js';

// The `theme` and `locale` cookies written by the server (008 RF-D4), with the attributes the
// client writes them with (RF-A4, ADR-0009 D3): never `HttpOnly`, because the client writes
// them too. `Secure` is Kit's default (on, except `http://localhost`).

export function setThemeCookie(cookies: Cookies, theme: ThemeChoice): void {
  cookies.set(THEME_COOKIE, theme, {
    path: '/',
    maxAge: THEME_COOKIE_MAX_AGE,
    sameSite: 'lax',
    httpOnly: false,
  });
}

export function setLocaleCookie(cookies: Cookies, locale: Locale): void {
  cookies.set(LOCALE_COOKIE, locale, {
    path: '/',
    maxAge: LOCALE_COOKIE_MAX_AGE,
    sameSite: 'lax',
    httpOnly: false,
  });
}
