// Blacklist sanitizer for `error_message` and `stack` (RF-A19). Removes:
// - the value that follows a sensitive header name;
// - credentials in URLs (`user:pass@`);
// - the query string of any URL, absolute or relative;
// - IPv6 addresses (full, compressed, with zone, with an embedded IPv4) and IPv4 addresses.

const SENSITIVE_HEADER =
  /(\b(?:cookie|authorization|x-forwarded-for|x-internal-secret|x-client-ip)["']?\s*[:=]\s*["']?)[^\r\n"']*/gi;

/** Userinfo of a URL (`scheme://user:pass@host`): everything between `//` and the `@`. */
const URL_CREDENTIALS = /(\b[a-z][a-z0-9+.-]*:\/\/)[^\s/?#@"'<>]+@/gi;

/**
 * A query string: a `?` that follows a URL or path character, or any `?name=` (even after a
 * space), and everything up to whitespace or a quote. Fragments go with it.
 */
const QUERY = /(?:(?<=[\w/.%~-])\?(?=[^\s"'<>`])|\?(?=[\w.~%-]+=))[^\s"'<>`]*/g;

const HEX = '[0-9a-f]{1,4}';
const IPV4_PART = '(?:\\d{1,3}\\.){3}\\d{1,3}';

/**
 * IPv6 that ends in a dotted IPv4 (`::ffff:203.0.113.77`, `64:ff9b::1.2.3.4`,
 * `0:0:0:0:0:ffff:1.2.3.4`). Matched first and as a whole: otherwise the IPv6 pattern eats the
 * `::ffff:203` prefix and leaves `.0.113.77`, which the IPv4 pattern no longer recognizes.
 */
const IPV6_EMBEDDED_IPV4 = new RegExp(
  [`(?:${HEX}:){6}${IPV4_PART}`, `(?:${HEX}(?::${HEX})*)?::(?:${HEX}:){0,5}${IPV4_PART}`]
    .map((alt) => `(?:${alt})`)
    .join('|') + '(?:%[0-9a-z._~-]+)?(?![\\d.])',
  'gi',
);

const IPV6 = new RegExp(
  [
    // full form: 8 groups
    `(?:${HEX}:){7}${HEX}`,
    // compressed form: anything with `::`
    `(?:${HEX}(?::${HEX})*)?::(?:${HEX}(?::${HEX})*)?`,
  ]
    .map((alt) => `(?:${alt})`)
    .join('|') + '(?:%[0-9a-z._~-]+)?',
  'gi',
);

const IPV4 = /\b(?:\d{1,3}\.){3}\d{1,3}\b/g;

export const REDACTED = '[redacted]';
export const REDACTED_IP = '[ip]';

export function sanitize(text: string): string {
  return text
    .replace(SENSITIVE_HEADER, `$1${REDACTED}`)
    .replace(URL_CREDENTIALS, `$1${REDACTED}@`)
    .replace(QUERY, '')
    .replace(IPV6_EMBEDDED_IPV4, REDACTED_IP)
    .replace(IPV6, (match) => (match === '::' ? match : REDACTED_IP))
    .replace(IPV4, REDACTED_IP);
}
