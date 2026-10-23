// Blacklist sanitizer for `error_message` and `stack` (RF-A19). Removes:
// - the value that follows a sensitive header name, also when it is an array or a `Map` entry;
// - credentials in URLs (`user:pass@`), even with an unencoded `/` or `@` in the password;
// - the query string and the fragment of any URL, absolute or relative;
// - IPv6 addresses (full, compressed, with zone, with an embedded IPv4) and IPv4 addresses.
//
// The input is cut to MAX_INPUT first, and every pattern is linear on adversarial input: a log
// line must never block the event loop (TA-10).

/** Longest input the sanitizer looks at (8 KB of UTF-16 code units). */
export const MAX_INPUT = 8 * 1024;
export const TRUNCATED = '[truncated]';

const HEADER_NAMES = 'cookie|authorization|x-forwarded-for|x-internal-secret|x-client-ip';

/**
 * A sensitive header name, its separator (`:`, `=` or the `=>` of `util.inspect(new Map())`) and
 * its value: an array up to `]` (any number of lines), or a plain or quoted string up to the quote
 * or the end of the line.
 */
const SENSITIVE_HEADER = new RegExp(
  `(\\b(?:${HEADER_NAMES})["']?\\s*(?:=>|[:=])\\s*)(?:\\[[^\\]]*\\]?|(["']?)[^\\r\\n"']*)`,
  'gi',
);

/**
 * Userinfo of a URL (`scheme://user:pass@host`). With a password, everything up to the last `@`
 * within 1 KB (so `/` and `@` in the password are covered); a bare user stops at `/?#@`. An empty
 * authority (`file:///…`) never matches. The scheme is bounded so `a.a.a.…` stays linear.
 */
const URL_CREDENTIALS =
  /(\b[a-z][a-z0-9+.-]{0,31}:\/\/)(?:[^\s/?#@:"'<>`]*:[^\s"'<>`]{0,1024}|[^\s/?#@:"'<>`]+)@/gi;

/**
 * A query string: a `?` that follows a URL or path character, or any `?name=` (even after a
 * space), and everything up to whitespace or a quote. Fragments go with it.
 */
const QUERY = /(?:(?<=[\w/.%~-])\?(?=[^\s"'<>`])|\?(?=[\w.~%-]+=))[^\s"'<>`]*/g;

/**
 * A fragment without a query: a `#` glued to a URL or path character (not `.`, so the private
 * members of stack frames, `Foo.#bar`, survive), or any `#name=`.
 */
const FRAGMENT = /(?:(?<=[\w/%~-])#(?=[^\s"'<>`])|#(?=[\w.~%-]+=))[^\s"'<>`]*/g;

const HEX = '[0-9a-f]{1,4}';
const IPV4_PART = '(?:\\d{1,3}\\.){3}\\d{1,3}';
/**
 * An IPv6 never starts right after a hex digit or a `:`. Without this anchor the compressed form
 * is retried from every position of a long `a:a:a:…` run, which is quadratic.
 */
const IPV6_START = '(?<![0-9a-f:])';

/**
 * IPv6 that ends in a dotted IPv4 (`::ffff:203.0.113.77`, `64:ff9b::1.2.3.4`,
 * `0:0:0:0:0:ffff:1.2.3.4`). Matched first and as a whole: otherwise the IPv6 pattern eats the
 * `::ffff:203` prefix and leaves `.0.113.77`, which the IPv4 pattern no longer recognizes.
 */
const IPV6_EMBEDDED_IPV4 = new RegExp(
  IPV6_START +
    '(?:' +
    [`(?:${HEX}:){6}${IPV4_PART}`, `(?:${HEX}(?::${HEX})*)?::(?:${HEX}:){0,5}${IPV4_PART}`]
      .map((alt) => `(?:${alt})`)
      .join('|') +
    ')(?:%[0-9a-z._~-]+)?(?![\\d.])',
  'gi',
);

const IPV6 = new RegExp(
  IPV6_START +
    '(?:' +
    [
      // full form: 8 groups
      `(?:${HEX}:){7}${HEX}`,
      // compressed form: anything with `::`
      `(?:${HEX}(?::${HEX})*)?::(?:${HEX}(?::${HEX})*)?`,
    ]
      .map((alt) => `(?:${alt})`)
      .join('|') +
    ')(?:%[0-9a-z._~-]+)?',
  'gi',
);

/** IPv4, also glued to a letter or `_` (`client_203.0.113.5`); only digits delimit it. */
const IPV4 = /(?<!\d)(?:\d{1,3}\.){3}\d{1,3}(?!\d)/g;

export const REDACTED = '[redacted]';
export const REDACTED_IP = '[ip]';

/**
 * Cuts the input to MAX_INPUT. The last token is dropped as well: the cut may split an address,
 * a credential or a query so that no pattern recognizes what is left of it.
 */
function truncate(text: string): string {
  if (text.length <= MAX_INPUT) return text;
  // A backwards scan, not `/\S*$/`: that regex is quadratic on a long run without whitespace.
  let end = MAX_INPUT;
  while (end > 0 && !/\s/.test(text[end - 1] ?? '')) end--;
  return `${text.slice(0, end)}${TRUNCATED}`;
}

export function sanitize(text: string): string {
  return truncate(text)
    .replace(SENSITIVE_HEADER, (_match, prefix: string, quote: string | undefined) =>
      quote === undefined ? `${prefix}${REDACTED}` : `${prefix}${quote}${REDACTED}`,
    )
    .replace(URL_CREDENTIALS, `$1${REDACTED}@`)
    .replace(QUERY, '')
    .replace(FRAGMENT, '')
    .replace(IPV6_EMBEDDED_IPV4, REDACTED_IP)
    .replace(IPV6, (match) => (match === '::' ? match : REDACTED_IP))
    .replace(IPV4, REDACTED_IP);
}
