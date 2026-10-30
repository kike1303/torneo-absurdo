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

/** `/dev/**` is enabled only with exactly `'1'` (RF-A16). Any other value means off. */
export function validateEnableDevCatalog(value: string | undefined): '1' | undefined {
  return value === '1' ? '1' : undefined;
}

/**
 * Whether `origin` is `http` on a loopback host (`localhost`, `127.0.0.1` or `[::1]`), written as
 * browsers send it. The same rule as the server's `is_loopback_http_origin` (spec 008, RF-B18):
 * both run the table of `scripts/__fixtures__/loopback-origins.json`. Anything else, `https`
 * included, is "outside loopback". Not wired to `env.ts` until phase D of 008.
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
