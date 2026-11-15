import { error, redirect } from '@sveltejs/kit';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { getFriendFight } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The replay of a fight between friends (023 "Web"), private: only with a session, and a 404 for
// anybody who did not fight it (or play its tournament). The SSR says who fought, never the result:
// the browser reads the log, the seed and the specs itself, so it checks what came straight from
// the server (011).
export const load: PageServerLoad = async ({ fetch, params, url, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  const id = parseFightId(params.id);
  if (id === null) error(404);
  const locale = localeFromPathname(url.pathname) ?? 'en';
  if (!locals.user) {
    if (locals.session === 'unavailable') return { status: 'unavailable' as const, id };
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/friends/fights/${id}`)}`);
  }
  const result = await getFriendFight(internalApi(fetch), id);
  if (!result.ok) {
    if ('status' in result && result.status === 404) error(404);
    return { status: 'unavailable' as const, id };
  }
  return { status: 'ready' as const, id, a: result.data.fight.a, b: result.data.fight.b };
};
