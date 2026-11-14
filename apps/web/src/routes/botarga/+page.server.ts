import { fail, redirect } from '@sveltejs/kit';
import type { ErrorCode } from '@torneo/protocol';
import { parseBuildForm, parseClientKey, parseDrawForm } from '#lib/features/botarga/build-form.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import {
  postBotarga,
  postDrawChoice,
  postPvpFight,
  putBotargaBuild,
} from '#lib/server/api/mutations.js';
import { getBotarga } from '#lib/server/api/reads.js';
import { apiFailure, noSession } from '#lib/server/session-guard.js';
import type { Actions, PageServerLoad } from './$types';

// My botarga (021 "Web"): only with a session. Every change is a form action, and the server
// decides every rule: the page only says what it answered. A failure of the API is a 200 with its
// own state, never a 500. The key of "search fight" is new on each load, and a retry of the same
// form (a reload of the POST) sends the same one, so it never makes a second fight.

export const load: PageServerLoad = async ({ fetch, url, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  const locale = localeFromPathname(url.pathname) ?? 'en';
  if (!locals.user) {
    if (locals.session === 'unavailable') return { state: 'unavailable' as const };
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/botarga`)}`);
  }
  const result = await getBotarga(internalApi(fetch));
  if (!result.ok) {
    if ('status' in result && result.status === 404) return { state: 'none' as const };
    return { state: 'error' as const };
  }
  return {
    state: 'ready' as const,
    botarga: result.data,
    clientKey: crypto.randomUUID(),
  };
};

const invalid = () => fail(422, { code: 'botarga.invalid_request' as ErrorCode });

/** What every action does before its own call: the session, or why there is none. */
function guard(locals: App.Locals, pathname: string) {
  if (locals.user) return null;
  if (locals.session === 'none') {
    const locale = localeFromPathname(pathname) ?? 'en';
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/botarga`)}`);
  }
  return noSession(locals.session);
}

export const actions: Actions = {
  create: async ({ url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const result = await postBotarga(internalApi(fetch));
    if (!result.ok) return apiFailure(result);
    return { saved: 'created' as const };
  },

  build: async ({ request, url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const body = parseBuildForm(await request.formData());
    if (body === null) return invalid();
    const result = await putBotargaBuild(internalApi(fetch), body);
    if (!result.ok) return apiFailure(result);
    return { saved: 'build' as const };
  },

  choose: async ({ request, url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const body = parseDrawForm(await request.formData());
    if (body === null) return invalid();
    const result = await postDrawChoice(internalApi(fetch), body);
    if (!result.ok) return apiFailure(result);
    return { saved: 'draw' as const };
  },

  attack: async ({ request, url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const key = parseClientKey(await request.formData());
    if (key === null) return fail(422, { code: 'pvp.invalid_request' as ErrorCode });
    const result = await postPvpFight(internalApi(fetch), { client_key: key });
    if (!result.ok) return apiFailure(result);
    const { fight, reward } = result.data;
    return {
      attacked: {
        id: fight.id,
        a: fight.a,
        b: fight.b,
        reward,
      },
    };
  },
};
