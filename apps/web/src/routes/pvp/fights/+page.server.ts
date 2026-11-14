import { redirect } from '@sveltejs/kit';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import { getPvpFights } from '#lib/server/api/reads.js';
import type { PageServerLoad } from './$types';

// My player fights (021 "Web"): only with a session, as attacker and as defender. `before` reaches
// the read only as a number. A failure of the API is a 200 with the error state, never a 500.
export const load: PageServerLoad = async ({ fetch, url, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  const locale = localeFromPathname(url.pathname) ?? 'en';
  if (!locals.user) {
    if (locals.session === 'unavailable') return { state: 'unavailable' as const };
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/pvp/fights`)}`);
  }
  const before = parseFightId(url.searchParams.get('before') ?? '');
  const page = await getPvpFights(internalApi(fetch), before);
  return {
    state: 'ready' as const,
    fights: page.ok ? page.data.fights : null,
    nextBefore: page.ok ? page.data.next_before : null,
  };
};
