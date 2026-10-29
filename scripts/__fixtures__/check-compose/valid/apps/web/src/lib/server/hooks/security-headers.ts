import type { Handle } from '@sveltejs/kit/hooks';

// RF-A10. HSTS without includeSubDomains or preload, so other subdomains of the owner are unaffected.
export const SECURITY_HEADERS = {
  'x-content-type-options': 'nosniff',
  'referrer-policy': 'strict-origin-when-cross-origin',
  'strict-transport-security': 'max-age=31536000',
  // ADR-0012 B2. The OAuth flow is full-page navigation with 303s and no popups (ADR-0008 D2).
  'cross-origin-opener-policy': 'same-origin',
  // ADR-0012 B2: `fullscreen` keeps its default (self) for the fight view.
  'permissions-policy':
    'camera=(), microphone=(), geolocation=(), payment=(), usb=(), display-capture=()',
} as const;

/** Adds the headers; if the response has immutable headers, returns a copy instead of failing. */
export function withSecurityHeaders(response: Response): Response {
  try {
    for (const [name, value] of Object.entries(SECURITY_HEADERS)) response.headers.set(name, value);
    return response;
  } catch (error) {
    if (!(error instanceof TypeError)) throw error;
  }
  const copy = new Response(response.body, response);
  for (const [name, value] of Object.entries(SECURITY_HEADERS)) copy.headers.set(name, value);
  return copy;
}

/** Outermost handle: does nothing before `resolve` and never logs (plan-a decision 3). */
export const securityHeaders: Handle = async ({ event, resolve }) =>
  withSecurityHeaders(await resolve(event));
