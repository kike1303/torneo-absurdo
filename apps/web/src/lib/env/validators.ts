// Pure validators for `src/env.ts` (plan-a decision 19). No Kit imports, so Vitest can load them.
// Error messages name the variable and the rule, never the value: Kit prints them at startup.

export const PUBLIC_ORIGIN_MISSING = 'PUBLIC_ORIGIN is required at runtime';
export const PUBLIC_ORIGIN_INVALID =
  'PUBLIC_ORIGIN must be an http(s) origin without path, query, fragment, credentials or trailing slash, in the form browsers send (lowercase host, no default port)';

/**
 * `http(s)://host[:port]` exactly as browsers serialize `Origin` (RF-A23, RF-B8). Optional only
 * while building; missing or invalid at runtime stops the app from starting.
 */
export function validatePublicOrigin(
  value: string | undefined,
  building: boolean,
): string | undefined {
  if (value === undefined || value === '') {
    if (building) return undefined;
    throw new Error(PUBLIC_ORIGIN_MISSING);
  }
  if (!isBrowserOrigin(value)) throw new Error(PUBLIC_ORIGIN_INVALID);
  return value;
}

function isBrowserOrigin(value: string): boolean {
  if (!URL.canParse(value)) return false;
  const parsed = new URL(value);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return false;
  if (parsed.username !== '' || parsed.password !== '') return false;
  // `origin` drops path, query, fragment and default port and lowercases the host; any of those
  // in the raw value (or a trailing slash) makes the two strings differ.
  return parsed.origin === value;
}

export const INTERNAL_API_URL_MISSING = 'INTERNAL_API_URL is required at runtime';
export const INTERNAL_API_URL_INVALID =
  'INTERNAL_API_URL must be an http(s) origin without path, query, fragment, credentials or trailing slash';
export const INTERNAL_API_URL_IS_PUBLIC =
  'env_invalid: INTERNAL_API_URL must not be on the host of PUBLIC_ORIGIN nor on a subdomain of it, whatever the port (Kit would forward every browser cookie to it)';
export const INTERNAL_API_SECRET_MISSING = 'INTERNAL_API_SECRET is required at runtime';
export const INTERNAL_API_SECRET_INVALID =
  'INTERNAL_API_SECRET must be base64url (padding optional) that decodes to at least 32 bytes';
export const INTERNAL_API_SECRET_IS_DEV =
  'env_invalid: INTERNAL_API_SECRET must not be the development value outside a loopback http PUBLIC_ORIGIN';

/**
 * Public development value of `apps/web/.env.example` and `apps/server/.env.example` (spec 008,
 * RF-B18). Outside a loopback `http` origin both processes refuse it.
 */
export const DEV_INTERNAL_API_SECRET = 'Y3NJpi4_9UAo8Kx01a81pv33Dleek_Ax2e1xq4kLJpk';
const INTERNAL_SECRET_MIN_BYTES = 32;

/**
 * `INTERNAL_API_URL` (ADR-0008 D5): the origin of the server for internal requests. Returned
 * serialized by WHATWG (`new URL(v).origin`: lowercase host, no default port), the form
 * `Request.url` takes, because `handleFetch` compares by prefix (008 decision 2).
 */
export function validateInternalApiUrl(
  value: string | undefined,
  building: boolean,
): string | undefined {
  if (value === undefined || value === '') {
    if (building) return undefined;
    throw new Error(INTERNAL_API_URL_MISSING);
  }
  if (!URL.canParse(value)) throw new Error(INTERNAL_API_URL_INVALID);
  const parsed = new URL(value);
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') {
    throw new Error(INTERNAL_API_URL_INVALID);
  }
  if (parsed.username !== '' || parsed.password !== '') throw new Error(INTERNAL_API_URL_INVALID);
  // Only the scheme, host and port may be written: no path (not even `/`), query or fragment.
  const rest = value.slice(value.indexOf('//') + 2);
  if (/[/?#\\]/.test(rest) || parsed.pathname !== '/') throw new Error(INTERNAL_API_URL_INVALID);
  return parsed.origin;
}

/**
 * The bytes of base64url text (padding optional), or `undefined` if it is not canonical
 * base64url: the same rule as the server's decoder (`DecodePaddingMode::Indifferent`, no
 * trailing bits), so both processes accept exactly the same secrets.
 */
export function decodeBase64Url(text: string): Uint8Array | undefined {
  const match = /^([A-Za-z0-9_-]*)(={0,2})$/.exec(text);
  if (!match) return undefined;
  const [, data = '', padding = ''] = match;
  if (data.length % 4 === 1) return undefined;
  if (padding !== '' && (data.length + padding.length) % 4 !== 0) return undefined;
  const bytes = Buffer.from(data, 'base64url');
  // Re-encoding gives back the same text only if the unused trailing bits were zero.
  if (bytes.toString('base64url') !== data) return undefined;
  return new Uint8Array(bytes);
}

/**
 * `INTERNAL_API_SECRET` (RF-B18, RF-D1): base64url of at least 32 bytes. The comparison with
 * the development value needs `PUBLIC_ORIGIN`, so it lives in {@link checkInternalApiEnv}.
 */
export function validateInternalApiSecret(
  value: string | undefined,
  building: boolean,
): string | undefined {
  if (value === undefined || value === '') {
    if (building) return undefined;
    throw new Error(INTERNAL_API_SECRET_MISSING);
  }
  const bytes = decodeBase64Url(value);
  if (!bytes || bytes.length < INTERNAL_SECRET_MIN_BYTES) {
    throw new Error(INTERNAL_API_SECRET_INVALID);
  }
  return value;
}

function sameBytes(a: Uint8Array, b: Uint8Array): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return diff === 0;
}

/**
 * Kit's server `fetch` copies the browser's cookies to a URL whose host is the site's host or a
 * subdomain of it, ports aside (`runtime/server/fetch.js`). Both values are serialized origins.
 */
function sharesCookies(internalApiUrl: string, publicOrigin: string): boolean {
  const internal = new URL(internalApiUrl).hostname;
  const site = new URL(publicOrigin).hostname;
  return `.${internal}`.endsWith(`.${site}`);
}

export interface InternalApiEnv {
  publicOrigin: string | undefined;
  internalApiUrl: string | undefined;
  internalApiSecret: string | undefined;
}

/**
 * The checks that compare two variables (008 decision 1). Kit 3 hands each `schema` only its
 * own value, and `vite dev` reads `.env` through `loadEnv`, not `process.env`, so a schema
 * cannot see `PUBLIC_ORIGIN`: `init` of `hooks.server.ts` runs this before the app serves. The
 * messages start with `env_invalid` and name the variable, never a value. Skipped while
 * building, when the values are `undefined`.
 */
export function checkInternalApiEnv(env: InternalApiEnv): void {
  const { publicOrigin, internalApiUrl, internalApiSecret } = env;
  if (publicOrigin === undefined) return;
  if (internalApiUrl !== undefined && sharesCookies(internalApiUrl, publicOrigin)) {
    throw new Error(INTERNAL_API_URL_IS_PUBLIC);
  }
  if (internalApiSecret !== undefined && !isLoopbackHttpOrigin(publicOrigin)) {
    const bytes = decodeBase64Url(internalApiSecret);
    const dev = decodeBase64Url(DEV_INTERNAL_API_SECRET);
    // Decoded bytes, not text: `DEV + "="` is the same secret (plan-b deviation 23).
    if (bytes && dev && sameBytes(bytes, dev)) throw new Error(INTERNAL_API_SECRET_IS_DEV);
  }
}

/** `/dev/**` is enabled only with exactly `'1'` (RF-A16). Any other value means off. */
export function validateEnableDevCatalog(value: string | undefined): '1' | undefined {
  return value === '1' ? '1' : undefined;
}

/**
 * Whether `origin` is `http` on a loopback host (`localhost`, `127.0.0.1` or `[::1]`), written as
 * browsers send it. The same rule as the server's `is_loopback_http_origin` (spec 008, RF-B18):
 * both run the table of `scripts/__fixtures__/loopback-origins.json`. Anything else, `https`
 * included, is "outside loopback".
 */
export function isLoopbackHttpOrigin(origin: string): boolean {
  if (!isBrowserOrigin(origin)) return false;
  const parsed = new URL(origin);
  return (
    parsed.protocol === 'http:' &&
    (parsed.hostname === 'localhost' ||
      parsed.hostname === '127.0.0.1' ||
      parsed.hostname === '[::1]')
  );
}
