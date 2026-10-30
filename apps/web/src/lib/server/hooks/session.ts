import type { Handle } from '@sveltejs/kit/hooks';
import { isExcludedPath } from '#lib/i18n/locale.js';
import type { Api } from '#lib/server/api/client.js';
import { getMe } from '#lib/server/api/reads.js';
import { DEV_SESSION_COOKIE, sessionCookieNames, sessionCookies } from './handle-fetch.js';

export type SessionState = 'none' | 'valid' | 'unavailable';

export interface SessionHandleConfig {
  publicOrigin: string;
  /** The internal API over `event.fetch` (so `handleFetch` adds the cookie and the secret). */
  api: (fetch: typeof globalThis.fetch) => Api;
}

const PRIVATE_NO_STORE = 'private, no-store';

/** Whether the browser sent a cookie with a session name of the policy, valid or not. */
function hasSessionCookieName(header: string | null, publicOrigin: string): boolean {
  if (!header) return false;
  const names = new Set(sessionCookieNames(publicOrigin));
  return header.split(';').some((part) => names.has(part.split('=')[0]?.trim() ?? ''));
}

/** `Cache-Control: private, no-store` (replacing any other) and `Cookie` in `Vary`. */
function privateResponse(response: Response): Response {
  let out = response;
  try {
    out.headers.set('cache-control', PRIVATE_NO_STORE);
  } catch {
    // `Response.redirect()` and fetched responses have immutable headers.
    out = new Response(response.body, response);
    out.headers.set('cache-control', PRIVATE_NO_STORE);
  }
  const vary = out.headers.get('vary');
  const names = (vary ?? '').split(',').map((v) => v.trim().toLowerCase());
  if (!names.includes('cookie') && !names.includes('*')) {
    out.headers.set('vary', vary ? `${vary}, Cookie` : 'Cookie');
  }
  return out;
}

/**
 * `locals.user` and `locals.session` (008 RF-D3, plan-d decision 4). With a session cookie of
 * RF-D2, one `GET /api/me`:
 * - 200: the user, `valid`;
 * - 401: no user, `none`, and the dead cookie is expired in the browser (the server's own
 *   `Set-Cookie` never reaches it through an internal fetch);
 * - anything else (network, timeout, 5xx, 429): no user, `unavailable`, the cookie untouched.
 *
 * Every response to a request that carried a session cookie name is `private, no-store` and
 * varies on `Cookie`. `/api`, `/ws` and `/healthz` are not touched.
 */
export function createSessionHandle(config: SessionHandleConfig): Handle {
  return async ({ event, resolve }) => {
    if (isExcludedPath(event.url.pathname)) return resolve(event);
    const header = event.request.headers.get('cookie');
    const cookies = sessionCookies(header, config.publicOrigin);
    event.locals.user = null;
    event.locals.session = 'none';
    if (cookies.length > 0) {
      const me = await getMe(config.api(event.fetch));
      if (me.ok) {
        event.locals.user = me.data;
        event.locals.session = 'valid';
      } else if ('status' in me && me.status === 401) {
        for (const [name] of cookies) {
          event.cookies.delete(name, {
            path: '/',
            httpOnly: true,
            sameSite: 'lax',
            secure: name !== DEV_SESSION_COOKIE,
          });
        }
      } else {
        event.locals.session = 'unavailable';
      }
    }
    const response = await resolve(event);
    return hasSessionCookieName(header, config.publicOrigin) ? privateResponse(response) : response;
  };
}
