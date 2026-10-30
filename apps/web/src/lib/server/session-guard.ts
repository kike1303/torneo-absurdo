import type { ErrorCode } from '@torneo/protocol';
import { fail } from '@sveltejs/kit';
import type { ApiResult } from '#lib/server/api/client.js';

/** The `fail()` of an API answer that was not a success, with a validated code (decision 9). */
export function apiFailure(result: Exclude<ApiResult<unknown>, { ok: true }>) {
  if ('unavailable' in result) return fail(503, { code: 'auth.try_again' as ErrorCode });
  return fail(result.status, { code: result.code ?? null });
}

/** The `fail()` of an action that needs a session and has none (`unavailable` or `none`). */
export function noSession(session: App.Locals['session']) {
  return session === 'unavailable'
    ? fail(503, { code: 'auth.try_again' as ErrorCode })
    : fail(401, { code: 'auth.unauthenticated' as ErrorCode });
}
