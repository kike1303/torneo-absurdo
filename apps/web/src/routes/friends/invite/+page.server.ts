import { fail } from '@sveltejs/kit';
import type { ErrorCode } from '@torneo/protocol';
import { parseInviteCode } from '#lib/features/friends/form.js';
import { internalApi } from '#lib/server/api/context.js';
import { postBotarga, postInviteAccept, postInvitePreview } from '#lib/server/api/mutations.js';
import { getBotarga } from '#lib/server/api/reads.js';
import { apiFailure, noSession } from '#lib/server/session-guard.js';
import type { Actions, PageServerLoad } from './$types';

// An invite (023 "Web"): `/friends/invite#<code>`. The code lives in the fragment and in the
// `sessionStorage` of the tab, never in a URL a server sees (nor in the `next` of the login). This
// load only says where the reader stands; the page posts the code to `preview` and, on a click,
// to `accept`. It never accepts on its own.

export const load: PageServerLoad = async ({ fetch, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  if (!locals.user) {
    return {
      state: locals.session === 'unavailable' ? ('unavailable' as const) : ('anonymous' as const),
    };
  }
  const botarga = await getBotarga(internalApi(fetch));
  if (!botarga.ok) {
    if ('status' in botarga && botarga.status === 404) return { state: 'none' as const };
    return { state: 'error' as const };
  }
  return { state: 'ready' as const };
};

const notFound = () => fail(404, { code: 'friends.invite_not_found' as ErrorCode });

export const actions: Actions = {
  create: async ({ locals, fetch }) => {
    if (!locals.user) return noSession(locals.session);
    const result = await postBotarga(internalApi(fetch));
    if (!result.ok) return apiFailure(result);
    return { created: true as const };
  },

  preview: async ({ request, locals, fetch }) => {
    if (!locals.user) return noSession(locals.session);
    const code = parseInviteCode(await request.formData());
    if (code === null) return notFound();
    const result = await postInvitePreview(internalApi(fetch), code);
    if (!result.ok) return apiFailure(result);
    return { preview: result.data.display_name };
  },

  accept: async ({ request, locals, fetch }) => {
    if (!locals.user) return noSession(locals.session);
    const form = await request.formData();
    const code = parseInviteCode(form);
    if (code === null) return notFound();
    const result = await postInviteAccept(internalApi(fetch), code);
    if (!result.ok) return apiFailure(result);
    const name = form.get('display_name');
    return { accepted: typeof name === 'string' ? name : '' };
  },
};
