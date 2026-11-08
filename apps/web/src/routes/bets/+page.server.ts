import { redirect } from '@sveltejs/kit';
import { historyRow } from '#lib/features/bets/history.js';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { getBetHistory } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// The bet history (019). Only with a session: without one, to the login and back here, with a fixed
// `next` and nothing of the query. `before` reaches the read only as a number. A failure of the API
// is a 200 with the error state, never a 500.
export const load: PageServerLoad = async ({ fetch, url, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  const locale = localeFromPathname(url.pathname) ?? 'en';
  if (!locals.user) {
    if (locals.session === 'unavailable') return { state: 'unavailable' as const };
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/bets`)}`);
  }
  const before = parseFightId(url.searchParams.get('before') ?? '');
  const page = await getBetHistory(internalApi(fetch), before);
  return {
    state: 'ready' as const,
    rows: page.ok ? page.data.items.map(historyRow) : null,
    nextBefore: page.ok ? page.data.next_before : null,
  };
};
