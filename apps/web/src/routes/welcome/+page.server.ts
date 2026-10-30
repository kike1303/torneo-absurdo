import { redirect } from '@sveltejs/kit';
import { safeNext } from '#lib/features/auth/next.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { setLocaleCookie, setThemeCookie } from '#lib/server/preference-cookies.js';
import type { PageServerLoad } from './$types';

/**
 * The `next` of every login (008 plan-d open question 4): only a `load` that reads `locals.user`
 * (from `GET /api/me`) and writes cookies, never an API mutation.
 *
 * - Without a session: the login.
 * - With one: the profile's `locale` and `theme` rewrite their cookies (RF-D4, "on login"; `null`
 *   leaves the cookie alone), then the onboarding while the alias is the generated one (deviation
 *   D3), or the validated `next`.
 *
 * The destination keeps the language of its own prefix even if the cookie was just rewritten
 * with the profile's: a prefixed route decides by its URL (RF-D4, change 7 of architect). This is
 * intentional.
 */
export const load: PageServerLoad = ({ url, locals, cookies }) => {
  const locale = localeFromPathname(url.pathname) ?? 'en';
  const user = locals.user;
  if (!user) redirect(303, `/${locale}/login`);
  if (user.locale !== null) setLocaleCookie(cookies, user.locale);
  if (user.theme !== null) setThemeCookie(cookies, user.theme);
  const next = safeNext(url.searchParams.get('next'), locale);
  if (user.display_name_is_generated) {
    redirect(303, `/${locale}/onboarding?next=${encodeURIComponent(next)}`);
  }
  redirect(303, next);
};
