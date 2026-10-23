import type { Handle } from '@sveltejs/kit/hooks';

const SAFE_METHODS = new Set(['GET', 'HEAD', 'OPTIONS']);

/** Body of the rejection, with the code of ADR-0008 D4. */
export const CSRF_ORIGIN_BODY = '{"error":"auth.csrf_origin"}';

function isTrusted(headers: Headers, trustedOrigin: string | undefined): boolean {
  // Fail closed: without a configured origin nothing unsafe passes (plan-a decision 3).
  if (trustedOrigin === undefined || trustedOrigin === '') return false;
  const origin = headers.get('origin');
  // Exact string comparison, never normalized: `Origin: null` or `https://TORNEO.example` fail.
  if (origin !== null) return origin === trustedOrigin;
  return headers.get('sec-fetch-site') === 'same-origin';
}

/**
 * RF-A23: rejects every method other than GET, HEAD and OPTIONS, on any route and with any
 * Content-Type, unless `Origin` is exactly `trustedOrigin` (or, without `Origin`,
 * `Sec-Fetch-Site: same-origin`). Never reads `Host`, `X-Forwarded-*` or `event.url`, never
 * calls `resolve` on rejection and never logs.
 */
export function createOriginGuard(trustedOrigin: string | undefined): Handle {
  return ({ event, resolve }) => {
    const { request } = event;
    if (SAFE_METHODS.has(request.method.toUpperCase())) return resolve(event);
    if (isTrusted(request.headers, trustedOrigin)) return resolve(event);
    return new Response(CSRF_ORIGIN_BODY, {
      status: 403,
      headers: { 'content-type': 'application/json' },
    });
  };
}
