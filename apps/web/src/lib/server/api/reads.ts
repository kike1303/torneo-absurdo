import type { FightsCurrent, Me, Provider, ReauthStatus } from '@torneo/protocol';
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

export function getProviders(api: Api): Promise<ApiResult<Provider[]>> {
  return apiRequest<Provider[]>(api, 'GET', '/api/auth/providers');
}

export function getReauthStatus(api: Api): Promise<ApiResult<ReauthStatus>> {
  return apiRequest<ReauthStatus>(api, 'GET', '/api/auth/reauth-status');
}
