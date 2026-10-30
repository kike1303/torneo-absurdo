import type { DisplayNameRequest, Me, PreferencesRequest, Provider } from '@torneo/protocol';
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
