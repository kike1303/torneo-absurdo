import type {
  BetPlaced,
  BetRequest,
  BotargaBuild,
  BotargaDrawChoice,
  BotargaView,
  DisplayNameRequest,
  FriendChallengeRequest,
  FriendFight,
  FriendInvitePreview,
  FriendTournament,
  FriendTournamentRequest,
  FriendsView,
  Me,
  PreferencesRequest,
  Provider,
  PvpFightCreated,
  PvpFightRequest,
  ShopBuyRequest,
  ShopPurchase,
  Wallet,
} from '@torneo/protocol';
import { apiRequest, type Api, type ApiResult } from './client.js';

// Mutations (008 plan-d decisions 3 and 8): only the `actions` of a `+page.server.ts` may call
// them. ESLint forbids the import anywhere else, and `tests/no-mutation-in-load.test.ts` checks
// that every call sits inside `export const actions`.

export function putDisplayName(api: Api, displayName: string): Promise<ApiResult<Me>> {
  const body: DisplayNameRequest = { display_name: displayName };
  return apiRequest<Me>(api, 'PUT', '/api/me/display-name', { body });
}

export function putPreferences(api: Api, body: PreferencesRequest): Promise<ApiResult<Me>> {
  return apiRequest<Me>(api, 'PUT', '/api/me/preferences', { body });
}

/** The server answers 303 on success (RF-C3); the action never follows it. */
export function postUnlink(api: Api, provider: Provider): Promise<ApiResult<undefined>> {
  return apiRequest<undefined>(api, 'POST', `/api/auth/unlink/${provider}`);
}

/** 201 for a new bet, 200 for a repeated `client_key` (012 RF25): both carry `BetPlaced`. */
export function postBet(api: Api, body: BetRequest): Promise<ApiResult<BetPlaced>> {
  return apiRequest<BetPlaced>(api, 'POST', '/api/bets', { body });
}

export function postBonusClaim(api: Api): Promise<ApiResult<Wallet>> {
  return apiRequest<Wallet>(api, 'POST', '/api/bonus/claim');
}

export function postBailoutClaim(api: Api): Promise<ApiResult<Wallet>> {
  return apiRequest<Wallet>(api, 'POST', '/api/bailout/claim');
}

/** Creates the botarga of a user who has none yet (021); 200 if it already had one. */
export function postBotarga(api: Api): Promise<ApiResult<BotargaView>> {
  return apiRequest<BotargaView>(api, 'POST', '/api/botarga');
}

export function putBotargaBuild(api: Api, body: BotargaBuild): Promise<ApiResult<BotargaView>> {
  return apiRequest<BotargaView>(api, 'PUT', '/api/botarga/build', { body });
}

export function postDrawChoice(api: Api, body: BotargaDrawChoice): Promise<ApiResult<BotargaView>> {
  return apiRequest<BotargaView>(api, 'POST', '/api/botarga/draw/choose', { body });
}

/** 201 for a new fight, 200 for a repeated `client_key` (021): both carry `PvpFightCreated`. */
export function postPvpFight(api: Api, body: PvpFightRequest): Promise<ApiResult<PvpFightCreated>> {
  // The fight is resolved by the engine in the request: more time than a read.
  return apiRequest<PvpFightCreated>(api, 'POST', '/api/pvp/fights', { body, timeoutMs: 10_000 });
}

/** A new invite code (023): the old one stops working. */
export function postInviteRotate(api: Api): Promise<ApiResult<FriendsView>> {
  return apiRequest<FriendsView>(api, 'POST', '/api/friends/invite/rotate');
}

/** Whose invite a code is (023): a POST, so the code never travels in a URL. */
export function postInvitePreview(api: Api, code: string): Promise<ApiResult<FriendInvitePreview>> {
  return apiRequest<FriendInvitePreview>(api, 'POST', '/api/friends/invite/preview', {
    body: { code },
  });
}

export function postInviteAccept(api: Api, code: string): Promise<ApiResult<FriendsView>> {
  return apiRequest<FriendsView>(api, 'POST', '/api/friends/accept', { body: { code } });
}

export function postFriendRemove(api: Api, displayName: string): Promise<ApiResult<FriendsView>> {
  return apiRequest<FriendsView>(api, 'POST', '/api/friends/remove', {
    body: { display_name: displayName },
  });
}

/** 201 for a new challenge, 200 for a repeated `client_key` (023): both carry `FriendFight`. */
export function postFriendChallenge(
  api: Api,
  body: FriendChallengeRequest,
): Promise<ApiResult<FriendFight>> {
  // The fight is resolved by the engine in the request: more time than a read.
  return apiRequest<FriendFight>(api, 'POST', '/api/friends/challenge', {
    body,
    timeoutMs: 10_000,
  });
}

/** 201 for a new tournament, 200 for a repeated `client_key` (023): both carry the bracket. */
export function postFriendTournament(
  api: Api,
  body: FriendTournamentRequest,
): Promise<ApiResult<FriendTournament>> {
  // Up to seven fights resolved by the engine in the request.
  return apiRequest<FriendTournament>(api, 'POST', '/api/friends/tournaments', {
    body,
    timeoutMs: 15_000,
  });
}

/** 201 for a purchase, 200 for a repeated `client_key` (024): both carry `ShopPurchase`. */
export function postShopBuy(api: Api, body: ShopBuyRequest): Promise<ApiResult<ShopPurchase>> {
  return apiRequest<ShopPurchase>(api, 'POST', '/api/shop/buy', { body });
}
