import { fail, redirect } from '@sveltejs/kit';
import { ENABLE_DEV_CATALOG } from '$app/env/private';
import type { ErrorCode, MyFightBets } from '@torneo/protocol';
import { isSide, parseBetForm } from '#lib/features/bets/form.js';
import { fightsToRead, targetFight } from '#lib/features/bets/target.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import type { ApiResult } from '#lib/server/api/client.js';
import { internalApi } from '#lib/server/api/context.js';
import { postBailoutClaim, postBet, postBonusClaim } from '#lib/server/api/mutations.js';
import {
  getFightsCurrent,
  getMyBets,
  getPool,
  getVoidStats,
  getWallet,
} from '#lib/server/api/reads.js';
import { noSession } from '#lib/server/session-guard.js';
import type { Actions, PageServerLoad } from './$types';

// The home page (010 RF53): what is on now, without a session. A failure is the error state of the
// view, with a 200, never a 500: the client still tries by itself. `debug` only exists next to the
// dev catalog, so production never gets those attributes (plan-d decision 14).
export const load: PageServerLoad = async ({ fetch, url, setHeaders, locals }) => {
  // What is on now: a cached board would show an old fight as the current one.
  setHeaders({ 'cache-control': 'no-store' });
  const api = internalApi(fetch);
  const result = await getFightsCurrent(api);
  const board = result.ok ? result.data : null;
  // The board comes from memory, on purpose. The counter touches the database: only when the last
  // fight needs it (012 RF22). Its failure only leaves out one line.
  const last = board?.last?.final ?? null;
  const target = targetFight(board);
  const signedIn = Boolean(locals.user);
  // 012 plan-c C1: with a session, the wallet and the bets of up to three fights, in parallel. A
  // failed read leaves its value out and never breaks the page (010 RF53).
  const ids = signedIn ? fightsToRead(board) : [];
  const [voidStats, pool, wallet, ...mine] = await Promise.all([
    last?.kind === 'voided' && last.reason === 'secret_lost' ? getVoidStats(api) : null,
    target === null ? null : getPool(api, target.id),
    signedIn ? getWallet(api) : null,
    ...ids.map((id) => getMyBets(api, id)),
  ]);
  const bets: Record<number, MyFightBets> = {};
  for (const read of mine) if (read?.ok) bets[read.data.fight_id] = read.data;
  return {
    board,
    voidStats: voidStats?.ok ? voidStats.data : null,
    debug: ENABLE_DEV_CATALOG === '1' && url.searchParams.get('debug') === '1',
    betting: {
      signedIn,
      wallet: wallet?.ok ? wallet.data : null,
      mine: bets,
      pool: pool?.ok ? pool.data : null,
      // RF44, plan-c C2: one key per read of the page, so posting the same form again never bets
      // twice, with or without JS.
      clientKey: crypto.randomUUID(),
    },
  };
};

/** The `fail()` of an API answer that is not a success, in the domain of the panel. */
function refused<Kept extends object>(
  result: Exclude<ApiResult<unknown>, { ok: true }>,
  fallback: ErrorCode,
  kept: Kept,
) {
  if ('unavailable' in result) return fail(503, { code: fallback, ...kept });
  return fail(result.status, { code: result.code ?? fallback, ...kept });
}

const loginPath = (pathname: string): string => {
  const locale = localeFromPathname(pathname) ?? 'en';
  return `/${locale}/login?next=${encodeURIComponent(`/${locale}/`)}`;
};

// 012 RF44: the mutations of the panel, by form action (ADR-0008 D5). They work without JS, and with
// `use:enhance` the page reads the wallet and the bets again after each one.
export const actions: Actions = {
  bet: async ({ request, url, locals, fetch }) => {
    const form = await request.formData();
    const typed = form.get('amount');
    const side = form.get('side');
    const bet = parseBetForm(form);
    const kept = {
      amount: typeof typed === 'string' ? typed.slice(0, 20) : '',
      side: isSide(side) ? side : null,
      fight: bet?.fight_id ?? null,
    };
    if (!locals.user) {
      if (locals.session === 'none') redirect(303, loginPath(url.pathname));
      return noSession(locals.session);
    }
    if (bet === null) return fail(422, { code: 'bet.invalid_request' as ErrorCode, ...kept });
    const result = await postBet(internalApi(fetch), bet);
    if (result.ok) return { placed: true };
    if ('status' in result && result.status === 401) redirect(303, loginPath(url.pathname));
    return refused(result, 'bet.unavailable', kept);
  },

  claimBonus: async ({ url, locals, fetch }) => {
    if (!locals.user) {
      if (locals.session === 'none') redirect(303, loginPath(url.pathname));
      return noSession(locals.session);
    }
    const result = await postBonusClaim(internalApi(fetch));
    if (result.ok) return { claimed: 'bonus' as const };
    if ('status' in result && result.status === 401) redirect(303, loginPath(url.pathname));
    return refused(result, 'wallet.unavailable', {});
  },

  claimBailout: async ({ url, locals, fetch }) => {
    if (!locals.user) {
      if (locals.session === 'none') redirect(303, loginPath(url.pathname));
      return noSession(locals.session);
    }
    const result = await postBailoutClaim(internalApi(fetch));
    if (result.ok) return { claimed: 'bailout' as const };
    if ('status' in result && result.status === 401) redirect(303, loginPath(url.pathname));
    return refused(result, 'wallet.unavailable', {});
  },
};
