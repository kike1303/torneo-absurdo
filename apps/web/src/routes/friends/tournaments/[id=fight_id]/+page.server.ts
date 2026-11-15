import { error, redirect } from '@sveltejs/kit';
import { hiddenView } from '#lib/features/friends/bracket.js';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { getFriendTournament } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// A tournament between friends (023 "Web"), private: only with a session, and a 404 for anybody
// who did not play it. The HTML carries who plays and round 1, never an outcome (011 RF18, RF20):
// the browser reads the whole tournament itself when the reader asks for the results.
export const load: PageServerLoad = async ({ fetch, params, url, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  const id = parseFightId(params.id);
  if (id === null) error(404);
  const locale = localeFromPathname(url.pathname) ?? 'en';
  if (!locals.user) {
    if (locals.session === 'unavailable') return { status: 'unavailable' as const, id };
    redirect(
      303,
      `/${locale}/login?next=${encodeURIComponent(`/${locale}/friends/tournaments/${id}`)}`,
    );
  }
  const result = await getFriendTournament(internalApi(fetch), id);
  if (!result.ok) {
    if ('status' in result && result.status === 404) error(404);
    return { status: 'unavailable' as const, id };
  }
  return { status: 'ready' as const, id, tournament: hiddenView(result.data) };
};
