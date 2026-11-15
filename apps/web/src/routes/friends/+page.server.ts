import { fail, redirect } from '@sveltejs/kit';
import type { ErrorCode } from '@torneo/protocol';
import { parseClientKey } from '#lib/features/botarga/build-form.js';
import { parseFriendName, parseTournamentForm } from '#lib/features/friends/form.js';
import { parseFightId } from '#lib/features/replay/fight-id.js';
import { localeFromPathname } from '#lib/i18n/locale.js';
import { internalApi } from '#lib/server/api/context.js';
import {
  postFriendChallenge,
  postFriendRemove,
  postFriendTournament,
  postInviteRotate,
} from '#lib/server/api/mutations.js';
import { getFriendFights, getFriendTournaments, getFriends } from '#lib/server/api/reads.js';
import { apiFailure, noSession } from '#lib/server/session-guard.js';
import type { Actions, PageServerLoad } from './$types';

// My friends (023 "Web"): only with a session and a botarga. Every change is a form action and the
// server decides every rule. The key of a challenge is new on each load, so a reload of the POST
// sends the same one and never makes a second fight. A failure of the API is a 200 with its own
// state, never a 500.

export const load: PageServerLoad = async ({ fetch, url, locals, setHeaders }) => {
  setHeaders({ 'cache-control': 'private, no-store' });
  const locale = localeFromPathname(url.pathname) ?? 'en';
  if (!locals.user) {
    if (locals.session === 'unavailable') return { state: 'unavailable' as const };
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/friends`)}`);
  }
  const api = internalApi(fetch);
  const before = parseFightId(url.searchParams.get('before') ?? '');
  const tbefore = parseFightId(url.searchParams.get('tbefore') ?? '');
  const [friends, fights, tournaments] = await Promise.all([
    getFriends(api),
    getFriendFights(api, before),
    getFriendTournaments(api, tbefore),
  ]);
  if (!friends.ok) {
    if ('status' in friends && friends.status === 404) return { state: 'none' as const };
    return { state: 'error' as const };
  }
  return {
    state: 'ready' as const,
    view: friends.data,
    fights: fights.ok ? fights.data.fights : null,
    nextBefore: fights.ok ? fights.data.next_before : null,
    // Without the champion: the list must not tell what the bracket hides (011 RF20).
    tournaments: tournaments.ok
      ? tournaments.data.tournaments.map(({ id, size, owner, created_at }) => ({
          id,
          size,
          owner,
          created_at,
        }))
      : null,
    tournamentsBefore: tournaments.ok ? tournaments.data.next_before : null,
    clientKey: crypto.randomUUID(),
    tournamentKey: crypto.randomUUID(),
  };
};

const invalid = () => fail(422, { code: 'friends.invalid_request' as ErrorCode });

/** What every action does before its own call: the session, or why there is none. */
function guard(locals: App.Locals, pathname: string) {
  if (locals.user) return null;
  if (locals.session === 'none') {
    const locale = localeFromPathname(pathname) ?? 'en';
    redirect(303, `/${locale}/login?next=${encodeURIComponent(`/${locale}/friends`)}`);
  }
  return noSession(locals.session);
}

export const actions: Actions = {
  rotate: async ({ url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const result = await postInviteRotate(internalApi(fetch));
    if (!result.ok) return apiFailure(result);
    return { saved: 'rotated' as const };
  },

  remove: async ({ request, url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const name = parseFriendName(await request.formData());
    if (name === null) return invalid();
    const result = await postFriendRemove(internalApi(fetch), name);
    if (!result.ok) return apiFailure(result);
    return { saved: 'removed' as const };
  },

  challenge: async ({ request, url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const form = await request.formData();
    const name = parseFriendName(form);
    const key = parseClientKey(form);
    if (name === null || key === null) return invalid();
    const result = await postFriendChallenge(internalApi(fetch), {
      display_name: name,
      client_key: key,
    });
    if (!result.ok) return apiFailure(result);
    const { fight } = result.data;
    return { challenged: { id: fight.id, a: fight.a, b: fight.b } };
  },

  tournament: async ({ request, url, locals, fetch }) => {
    const refused = guard(locals, url.pathname);
    if (refused) return refused;
    const body = parseTournamentForm(await request.formData());
    if (body === null) return fail(422, { code: 'friends.tournament_invalid' as ErrorCode });
    const result = await postFriendTournament(internalApi(fetch), body);
    if (!result.ok) return apiFailure(result);
    const locale = localeFromPathname(url.pathname) ?? 'en';
    redirect(303, `/${locale}/friends/tournaments/${result.data.id}`);
  },
};
