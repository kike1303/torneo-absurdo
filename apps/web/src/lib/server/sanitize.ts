// Blacklist sanitizer for `error_message` and `stack` (RF-A19). Removes:
// - the value that follows a sensitive header name: a whole quoted string (with escapes), a whole
//   array (also from a `Map`), or the rest of the line;
// - credentials in URLs (`user:pass@`), even with an unencoded `/` or `@` in the password;
// - the query string and the fragment of any URL, absolute or relative;
// - IPv6 addresses (full, compressed, with zone, with an embedded IPv4) and IPv4 addresses.
//
// The input is cut to MAX_INPUT first, and every pattern is linear on adversarial input: a log
// line must never block the event loop (TA-10).

/** Longest input the sanitizer looks at (8 KB of UTF-16 code units). */
export const MAX_INPUT = 8 * 1024;
export const TRUNCATED = '[truncated]';
export const REDACTED = '[redacted]';
export const REDACTED_IP = '[ip]';

const HEADER_NAMES = 'cookie|authorization|x-forwarded-for|x-internal-secret|x-client-ip';

/**
 * A sensitive header name, an optional closing quote (also escaped, `\"`, as in JSON inside a
 * string) and its separator: `:`, `=` or the `=>` of `util.inspect(new Map())`. The value that
 * follows is consumed by `redactHeaderValues()`, which reads its structure instead of stopping at
 * the first quote or `]` (PR #2 review).
 */
const SENSITIVE_HEADER = new RegExp(
  `\\b(?:${HEADER_NAMES})(?:\\\\?["'\`])?\\s*(?:=>|[:=])[ \\t]*`,
  'gi',
);

const QUOTES = new Set(["'", '"', '`']);

/**
 * End of a string that opens at `start` (after its closing quote), honoring `\\` escapes, or -1
 * if it never closes. `'` and `"` strings end at a line break, as in JS and JSON.
 */
function stringEnd(text: string, start: number): number {
  const quote = text[start];
  for (let i = start + 1; i < text.length; i++) {
    const ch = text[i];
    if (ch === '\\') i++;
    else if (ch === quote) return i + 1;
    else if (quote !== '`' && (ch === '\n' || ch === '\r')) return -1;
  }
  return -1;
}

/** End of an array that opens at `start` (after its closing `]`); strings may hold `]`. */
function arrayEnd(text: string, start: number): number {
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    const ch = text[i] ?? '';
    if (QUOTES.has(ch)) {
      const end = stringEnd(text, i);
      if (end < 0) return text.length;
      i = end - 1;
    } else if (ch === '[') depth++;
    else if (ch === ']' && --depth === 0) return i + 1;
  }
  return text.length;
}

/**
 * Redacts every sensitive header value in one left-to-right pass (linear: each value is read
 * once). An array goes whole; a quoted string goes whole and keeps its quotes; anything else
 * (including a value that opens with an escaped quote) goes to the end of the line. A string or
 * an array that never closes is redacted to the end of the input: when in doubt, redact more.
 */
function redactHeaderValues(text: string): string {
  let out = '';
  let last = 0;
  SENSITIVE_HEADER.lastIndex = 0;
  for (let match; (match = SENSITIVE_HEADER.exec(text)) !== null;) {
    const start = match.index + match[0].length;
    const first = text[start] ?? '';
    let end: number;
    let replacement = REDACTED;
    if (first === '[') {
      end = arrayEnd(text, start);
    } else if (QUOTES.has(first)) {
      const close = stringEnd(text, start);
      end = close < 0 ? text.length : close;
      replacement = close < 0 ? `${first}${REDACTED}` : `${first}${REDACTED}${first}`;
    } else {
      end = start;
      while (end < text.length && text[end] !== '\n' && text[end] !== '\r') end++;
    }
    out += text.slice(last, start) + replacement;
    last = end;
    SENSITIVE_HEADER.lastIndex = end;
  }
  return out + text.slice(last);
}

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
 * An IPv6 never starts right after a hex digit, or after a `:` that follows one. Without this
 * anchor the compressed form is retried from every position of a long `a:a:a:…` run, which is
 * quadratic. A `:` after a non-hex character still allows it, so `peer:2001:db8::1` and
 * `ip:::ffff:1.2.3.4` are redacted (PR #2 review).
 */
const IPV6_START = '(?<![0-9a-f]|[0-9a-f]:)';

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

/** Where a cut may fall: whitespace and the separators of logs, JSON and `util.inspect`. */
const CUT_AFTER = /[\s"'<>,;]/;

/**
 * Cuts the input to MAX_INPUT, back to the last separator: the cut may split an address, a
 * credential or a query so that no pattern recognizes what is left of it, so that last token is
 * dropped. A backwards scan, not `/\S*$/`, which is quadratic on a long run without separators.
 */
function truncate(text: string): string {
  if (text.length <= MAX_INPUT) return text;
  let end = MAX_INPUT;
  while (end > 0 && !CUT_AFTER.test(text[end - 1] ?? '')) end--;
  return `${text.slice(0, end)}${TRUNCATED}`;
}

export function sanitize(text: string): string {
  return redactHeaderValues(truncate(text))
    .replace(URL_CREDENTIALS, `$1${REDACTED}@`)
    .replace(QUERY, '')
    .replace(FRAGMENT, '')
    .replace(IPV6_EMBEDDED_IPV4, REDACTED_IP)
    .replace(IPV6, (match) => (match === '::' ? match : REDACTED_IP))
    .replace(IPV4, REDACTED_IP);
}
