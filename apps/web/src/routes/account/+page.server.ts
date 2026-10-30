import { fail, redirect } from '@sveltejs/kit';
import type { ErrorCode } from '@torneo/protocol';
import { isProvider, parseReauthStatus } from '#lib/features/account/providers.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { postUnlink, putDisplayName } from '#lib/server/api/mutations.js';
import { getProviders, getReauthStatus } from '#lib/server/api/reads.js';
import { apiFailure, noSession } from '#lib/server/session-guard.js';
import type { Actions, PageServerLoad } from './$types';

/** The only query /account keeps: the way back from a reauthentication (009 RF22). */
const REAUTH_DONE = '?reauth=1';

// The account (008 RF-D5, plan-d decision 9): alias, balance, providers and sessions. Rename and
// unlink are form actions (their errors become text); link, logout and logout-all post straight
// to /api, because the server has to set or expire cookies in the browser.

export const load: PageServerLoad = async ({ url, request, locals, fetch }) => {
  const locale = localeFromPathname(url.pathname) ?? 'en';
  // 009 RF23, decision 13: no value of the URL reaches the page or the locale switcher, which
  // copies `search`. Only a GET: without JS, Kit runs an action and then this load with the same
  // URL (`?/unlink`), and a redirect there would lose the `fail()` of the action.
  if (request.method === 'GET' && url.search !== '' && url.search !== REAUTH_DONE) {
    const query = url.searchParams.has('reauth') ? REAUTH_DONE : '';
    redirect(303, `/${locale}/account${query}`);
  }
  if (!locals.user) {
    if (locals.session === 'unavailable') return { state: 'unavailable' as const };
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/account`)}`);
  }
  const api = internalApi(fetch);
  const [enabled, status] = await Promise.all([getProviders(api), getReauthStatus(api)]);
  const reauth = status.ok ? parseReauthStatus(status.data) : null;
  return {
    state: 'ready' as const,
    displayName: locals.user.display_name,
    balance: locals.user.balance,
    linked: locals.user.providers,
    enabled: enabled.ok && Array.isArray(enabled.data) ? enabled.data.filter(isProvider) : null,
    reauth,
    // Only the presence of the parameter (RF23), and only if the confirmation is really there.
    reauthDone: url.searchParams.has('reauth') && reauth?.link_ready === true,
  };
};

export const actions: Actions = {
  rename: async ({ request, url, locals, fetch }) => {
    const locale = localeFromPathname(url.pathname) ?? 'en';
    const value = String((await request.formData()).get('display_name') ?? '');
    if (!locals.user) {
      if (locals.session === 'none') redirect(303, `/${locale}/login`);
      return noSession(locals.session);
    }
    const result = await putDisplayName(internalApi(fetch), value);
    if (result.ok) return { renamed: true };
    if ('status' in result && result.status === 401) redirect(303, `/${locale}/login`);
    const failure = apiFailure(result);
    return fail(failure.status, { ...failure.data, value });
  },

  unlink: async ({ request, url, locals, fetch }) => {
    const locale = localeFromPathname(url.pathname) ?? 'en';
    const provider = (await request.formData()).get('provider');
    if (!isProvider(provider)) return fail(422, { code: 'http.invalid_body' as ErrorCode });
    if (!locals.user) {
      if (locals.session === 'none') redirect(303, `/${locale}/login`);
      return noSession(locals.session);
    }
    const result = await postUnlink(internalApi(fetch), provider);
    if (result.ok) return { unlinked: true };
    if ('status' in result && result.status === 401) redirect(303, `/${locale}/login`);
    const failure = apiFailure(result);
    return fail(failure.status, { ...failure.data, provider });
  },
};
