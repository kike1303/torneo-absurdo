import type {
  BetHistoryPage,
  BotargaView,
  FightDetail,
  FightPool,
  FightsCurrent,
  FightsPage,
  FriendFight,
  FriendFightsPage,
  FriendTournament,
  FriendTournamentsPage,
  FriendsView,
  Leaderboard,
  Me,
  MyFightBets,
  Provider,
  PvpFightDetail,
  PvpFightsPage,
  PvpLeaderboard,
  ReauthStatus,
  SeasonView,
  VoidStats,
  Wallet,
} from '@torneo/protocol';
import { apiRequest, type Api, type ApiResult } from './client.js';

// Reads only (008 plan-d decisions 3 and 8): `GET`, usable from hooks and `load` functions.
// `tests/no-mutation-in-load.test.ts` fails if a call here uses another method.

export function getMe(api: Api): Promise<ApiResult<Me>> {
  return apiRequest<Me>(api, 'GET', '/api/me');
}

/** What is on now (010 RF35): from the memory of the server, without a session. */
export function getFightsCurrent(api: Api): Promise<ApiResult<FightsCurrent>> {
  return apiRequest<FightsCurrent>(api, 'GET', '/api/fights/current');
}

/**
 * A page of settled fights (011 RF7). By number, never a text of the URL: `handleFetch` adds the
 * internal secret to whatever path this builds (RF18).
 */
export function getFightsPage(api: Api, before: number | null): Promise<ApiResult<FightsPage>> {
  return apiRequest<FightsPage>(
    api,
    'GET',
    before === null ? '/api/fights' : `/api/fights?before=${before}`,
  );
}

/** A fight (010 RF34). By number, like `getFightsPage`. */
export function getFightDetail(api: Api, id: number): Promise<ApiResult<FightDetail>> {
  return apiRequest<FightDetail>(api, 'GET', `/api/fights/${id}`);
}

/** The public counter of the voided fights (012 RF23). */
export function getVoidStats(api: Api): Promise<ApiResult<VoidStats>> {
  return apiRequest<VoidStats>(api, 'GET', '/api/fights/void-stats');
}

export function getProviders(api: Api): Promise<ApiResult<Provider[]>> {
  return apiRequest<Provider[]>(api, 'GET', '/api/auth/providers');
}

export function getReauthStatus(api: Api): Promise<ApiResult<ReauthStatus>> {
  return apiRequest<ReauthStatus>(api, 'GET', '/api/auth/reauth-status');
}

/** The wallet of the session's user (012 RF32). */
export function getWallet(api: Api): Promise<ApiResult<Wallet>> {
  return apiRequest<Wallet>(api, 'GET', '/api/wallet');
}

/** The session's own bets on one fight (012 RF33). */
export function getMyBets(api: Api, fightId: number): Promise<ApiResult<MyFightBets>> {
  return apiRequest<MyFightBets>(api, 'GET', `/api/bets/mine?fight_id=${fightId}`);
}

/** A page of the session's own bet history (019). By number, like `getFightsPage`. */
export function getBetHistory(api: Api, before: number | null): Promise<ApiResult<BetHistoryPage>> {
  return apiRequest<BetHistoryPage>(
    api,
    'GET',
    before === null ? '/api/bets/history' : `/api/bets/history?before=${before}`,
  );
}

/** The public pool of one fight (012 RF34). */
export function getPool(api: Api, fightId: number): Promise<ApiResult<FightPool>> {
  return apiRequest<FightPool>(api, 'GET', `/api/fights/${fightId}/pool`);
}

/** A season's leaderboard (020), without a session. By number, like `getFightsPage`. */
export function getLeaderboard(api: Api, season: number | null): Promise<ApiResult<Leaderboard>> {
  return apiRequest<Leaderboard>(
    api,
    'GET',
    season === null ? '/api/leaderboard' : `/api/leaderboard?season=${season}`,
  );
}

/** The seasons with a row, the newest first (020). */
export function getSeasons(api: Api): Promise<ApiResult<SeasonView[]>> {
  return apiRequest<SeasonView[]>(api, 'GET', '/api/seasons');
}

/** The botarga of the session's user (021); 404 `botarga.not_created` until it has one. */
export function getBotarga(api: Api): Promise<ApiResult<BotargaView>> {
  return apiRequest<BotargaView>(api, 'GET', '/api/botarga');
}

/** The PvP fights of the session's user, as attacker and as defender (021). */
export function getPvpFights(api: Api, before: number | null): Promise<ApiResult<PvpFightsPage>> {
  const path = before === null ? '/api/pvp/fights' : `/api/pvp/fights?before=${before}`;
  return apiRequest<PvpFightsPage>(api, 'GET', path as `/api/${string}`);
}

/** The PvP leaderboard (021): the current season without `season`. */
export function getPvpLeaderboard(
  api: Api,
  season: number | null,
): Promise<ApiResult<PvpLeaderboard>> {
  const path = season === null ? '/api/pvp/leaderboard' : `/api/pvp/leaderboard?season=${season}`;
  return apiRequest<PvpLeaderboard>(api, 'GET', path as `/api/${string}`);
}

export function getPvpSeasons(api: Api): Promise<ApiResult<SeasonView[]>> {
  return apiRequest<SeasonView[]>(api, 'GET', '/api/pvp/seasons');
}

/** A player fight, public (021): who fought, for the title. The browser reads the rest itself. */
export function getPvpFightDetail(api: Api, id: number): Promise<ApiResult<PvpFightDetail>> {
  return apiRequest<PvpFightDetail>(api, 'GET', `/api/pvp/fights/${id}`);
}

/** My friends and my invite code (023); 404 `botarga.not_created` without a botarga. */
export function getFriends(api: Api): Promise<ApiResult<FriendsView>> {
  return apiRequest<FriendsView>(api, 'GET', '/api/friends');
}

/** My challenges and matches between friends (023), private. */
export function getFriendFights(
  api: Api,
  before: number | null,
): Promise<ApiResult<FriendFightsPage>> {
  const path = before === null ? '/api/friends/fights' : `/api/friends/fights?before=${before}`;
  return apiRequest<FriendFightsPage>(api, 'GET', path as `/api/${string}`);
}

/** A fight between friends (023): who fought, for the title; 404 for anybody else. */
export function getFriendFight(api: Api, id: number): Promise<ApiResult<FriendFight>> {
  return apiRequest<FriendFight>(api, 'GET', `/api/friends/fights/${id}`);
}

/** The tournaments I played (023), private. */
export function getFriendTournaments(
  api: Api,
  before: number | null,
): Promise<ApiResult<FriendTournamentsPage>> {
  const path =
    before === null ? '/api/friends/tournaments' : `/api/friends/tournaments?before=${before}`;
  return apiRequest<FriendTournamentsPage>(api, 'GET', path as `/api/${string}`);
}

/** A tournament and its bracket (023); 404 for anybody who did not play it. */
export function getFriendTournament(api: Api, id: number): Promise<ApiResult<FriendTournament>> {
  return apiRequest<FriendTournament>(api, 'GET', `/api/friends/tournaments/${id}`);
}
