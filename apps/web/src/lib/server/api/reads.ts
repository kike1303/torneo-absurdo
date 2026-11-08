import type {
  FightDetail,
  FightsCurrent,
  FightsPage,
  Me,
  Provider,
  ReauthStatus,
  VoidStats,
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
