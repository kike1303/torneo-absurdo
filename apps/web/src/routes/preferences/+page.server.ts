import { fail, redirect } from '@sveltejs/kit';
import type { ErrorCode } from '@torneo/protocol';
import { safeNext } from '#lib/features/auth/next.js';
import { isLocale, localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { putPreferences } from '#lib/server/api/mutations.js';
import { setLocaleCookie, setThemeCookie } from '#lib/server/preference-cookies.js';
import { apiFailure, noSession } from '#lib/server/session-guard.js';
import { parseTheme } from '#lib/theme/resolve-theme.js';
import type { Actions, PageServerLoad } from './$types';

// Theme and language with a session (008 RF-D4, plan-d decision 5): the profile through
// `PUT /api/me/preferences`, and the cookie only after a 200.

const invalid = () => fail(422, { code: 'http.invalid_body' as ErrorCode });

/** Only actions here: a `GET` goes to the account. */
export const load: PageServerLoad = ({ url }) => {
  redirect(303, `/${localeFromPathname(url.pathname) ?? 'en'}/account`);
};

export const actions: Actions = {
  /** Posted by the theme switcher with `fetch`; it already applied the theme live. */
  theme: async ({ request, locals, fetch, cookies }) => {
    const theme = parseTheme((await request.formData()).get('theme'));
    if (!theme) return invalid();
    if (!locals.user) return noSession(locals.session);
    const result = await putPreferences(internalApi(fetch), { theme });
    if (!result.ok) return apiFailure(result);
    setThemeCookie(cookies, theme);
    return { saved: true };
  },

  /** A plain form of the language switcher: always 303 to `to` (validated), no JavaScript. */
  locale: async ({ request, locals, fetch, cookies }) => {
    const form = await request.formData();
    const locale = form.get('locale');
    if (!isLocale(locale)) return invalid();
    const to = form.get('to');
    const destination = safeNext(typeof to === 'string' ? to : null, locale);
    if (locals.user) {
      const result = await putPreferences(internalApi(fetch), { locale });
      if (result.ok) setLocaleCookie(cookies, locale);
    } else {
      setLocaleCookie(cookies, locale);
    }
    redirect(303, destination);
  },
};
