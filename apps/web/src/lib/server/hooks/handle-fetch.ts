import type { HandleFetch } from '@sveltejs/kit/hooks';
import { isLoopbackHttpOrigin } from '#lib/env/validators.js';

/** The cookie of the production policy (008 RF-B9), forwarded always. */
export const SESSION_COOKIE = '__Host-session';
/** The loopback development cookie, forwarded only on an `http` loopback origin (RF-D2). */
export const DEV_SESSION_COOKIE = 'ta_dev_session';
/** What the server issues (base64url); anything else is never forwarded. */
const COOKIE_VALUE = /^[A-Za-z0-9_-]{1,128}$/;

/** Internal headers: only the web sets them, only towards INTERNAL_API_URL (ADR-0008 D5). */
const X_INTERNAL_SECRET = 'x-internal-secret';
const X_CLIENT_IP = 'x-client-ip';
const X_CLIENT_ORIGIN = 'x-client-origin';
const X_CLIENT_FETCH_SITE = 'x-client-fetch-site';

/** The names of the session cookies of RF-D2 for this `PUBLIC_ORIGIN`. */
export function sessionCookieNames(publicOrigin: string): string[] {
  return isLoopbackHttpOrigin(publicOrigin)
    ? [SESSION_COOKIE, DEV_SESSION_COOKIE]
    : [SESSION_COOKIE];
}

/**
 * The session cookies of the browser's `Cookie` header that the web may forward (RF-D2), as
 * `[name, value]`. Exact names only; a repeated cookie counts as absent (like the server), and a
 * value that is not base64url of 1 to 128 characters is dropped, so nothing from the browser can
 * smuggle another `;` or cookie through.
 */
export function sessionCookies(
  header: string | null | undefined,
  publicOrigin: string,
): [string, string][] {
  if (!header) return [];
  const found = new Map<string, string[]>();
  for (const part of header.split(';')) {
    const at = part.indexOf('=');
    if (at === -1) continue;
    const name = part.slice(0, at).trim();
    const values = found.get(name) ?? [];
    values.push(part.slice(at + 1).trim());
    found.set(name, values);
  }
  const out: [string, string][] = [];
  for (const name of sessionCookieNames(publicOrigin)) {
    const values = found.get(name);
    if (values?.length === 1 && COOKIE_VALUE.test(values[0] ?? '')) out.push([name, values[0]!]);
  }
  return out;
}

/**
 * Whether a serialized URL goes to INTERNAL_API_URL: it starts with the origin **and** the `/`
 * that follows it, so `…:8080@evil/`, `…:80800/` and `…:8080.evil/` never match.
 */
export function isInternalUrl(url: string, internalApiUrl: string): boolean {
  return url.startsWith(`${internalApiUrl}/`);
}

export interface HandleFetchConfig {
  /** Serialized origin, as `validateInternalApiUrl` returns it (no trailing slash). */
  internalApiUrl: string;
  secret: string;
  publicOrigin: string;
}

/** Removes `cookie`, `x-internal-secret` and every `x-client-*` header. */
function withoutInternalHeaders(source: Headers): Headers {
  const headers = new Headers(source);
  for (const name of [...headers.keys()]) {
    if (name === 'cookie' || name === X_INTERNAL_SECRET || name.startsWith('x-client-')) {
      headers.delete(name);
    }
  }
  return headers;
}

/**
 * `handleFetch` (008 RF-D1, plan-d decision 2). A request is internal only if its URL starts
 * with `${internalApiUrl}/`: `Request.url` is already serialized by WHATWG, and the `/` right
 * after the origin keeps out `…:8080@evil/`, `…:80800/` and `…:8080.evil/`.
 *
 * - Internal: the caller's headers minus every internal one, then the session cookies of
 *   RF-D2, the secret, `getClientAddress()` and a literal copy of the browser's `Origin` and
 *   `Sec-Fetch-Site` (absent if the browser did not send them), with `redirect: 'manual'`.
 * - Anything else: without `cookie`, the secret or any `x-client-*`, even if the caller set
 *   them (T16).
 *
 * Writes no log.
 */
export function createHandleFetch(config: HandleFetchConfig): HandleFetch {
  // `outgoing` is the request our own code built for `fetch`, not the browser's request: its
  // URL is ours (INTERNAL_API_URL or a constant), so comparing its text is not building an
  // absolute URL from client data (plan-a decision 20).
  return ({ event, request: outgoing, fetch }) => {
    const headers = withoutInternalHeaders(outgoing.headers);
    if (!isInternalUrl(outgoing.url, config.internalApiUrl)) {
      return fetch(new Request(outgoing, { headers }));
    }
    const browser = event.request.headers;
    const cookies = sessionCookies(browser.get('cookie'), config.publicOrigin);
    if (cookies.length > 0) {
      headers.set('cookie', cookies.map(([name, value]) => `${name}=${value}`).join('; '));
    }
    headers.set(X_INTERNAL_SECRET, config.secret);
    headers.set(X_CLIENT_IP, event.getClientAddress());
    const origin = browser.get('origin');
    if (origin !== null) headers.set(X_CLIENT_ORIGIN, origin);
    const fetchSite = browser.get('sec-fetch-site');
    if (fetchSite !== null) headers.set(X_CLIENT_FETCH_SITE, fetchSite);
    // A form action has to see the 303 of the server; a read never redirects.
    return fetch(new Request(outgoing, { headers, redirect: 'manual' }));
  };
}
