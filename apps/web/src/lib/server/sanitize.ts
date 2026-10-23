// Blacklist sanitizer for `error_message` and `stack` (RF-A19). Removes:
// - the value that follows a sensitive header name;
// - the query string of any URL, absolute or relative;
// - IPv6 addresses (full, compressed, with zone) and IPv4 addresses.

const SENSITIVE_HEADER =
  /(\b(?:cookie|authorization|x-forwarded-for|x-internal-secret|x-client-ip)["']?\s*[:=]\s*["']?)[^\r\n"']*/gi;

/**
 * A query string: a `?` that follows a URL or path character, or any `?name=` (even after a
 * space), and everything up to whitespace or a quote. Fragments go with it.
 */
const QUERY = /(?:(?<=[\w/.%~-])\?(?=[^\s"'<>`])|\?(?=[\w.~%-]+=))[^\s"'<>`]*/g;

const HEX = '[0-9a-f]{1,4}';
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
    .replace(QUERY, '')
    .replace(IPV6, (match) => (match === '::' ? match : REDACTED_IP))
    .replace(IPV4, REDACTED_IP);
}
