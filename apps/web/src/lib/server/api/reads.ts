import type { Me, Provider } from '@torneo/protocol';
import { apiRequest, type Api, type ApiResult } from './client.js';

// Reads only (008 plan-d decisions 3 and 8): `GET`, usable from hooks and `load` functions.
// `tests/no-mutation-in-load.test.ts` fails if a call here uses another method.

export function getMe(api: Api): Promise<ApiResult<Me>> {
  return apiRequest<Me>(api, 'GET', '/api/me');
}

export function getProviders(api: Api): Promise<ApiResult<Provider[]>> {
  return apiRequest<Provider[]>(api, 'GET', '/api/auth/providers');
}
