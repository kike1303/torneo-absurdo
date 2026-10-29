// Blacklist sanitizer for `stack` (RF-A19, ADR-0012 A3). Removes:
// - the value that follows a sensitive header name, also when it starts on the next line: a whole
//   quoted string (with escapes), a whole array or object (also from a `Map`), or the rest of the
//   line;
// - credentials in URLs (`user:pass@`), even with an unencoded `/` or `@` in the password;
// - the query string and the fragment of any URL, absolute or relative;
// - IPv6 addresses (full, compressed, with zone, with an embedded IPv4), also after a key that
//   looks like hex, and IPv4 addresses.
//
// The input is cut to MAX_INPUT first, and every pattern is linear on adversarial input: a log
// line must never block the event loop (TA-10).

/** Longest input the sanitizer looks at (8 KB of UTF-16 code units). */
export const MAX_INPUT = 8 * 1024;
export const TRUNCATED = '[truncated]';
export const REDACTED = '[redacted]';
export const REDACTED_IP = '[ip]';

/**
 * Sensitive header names. `forwarded`, `x-real-ip` and every `x-client-*` come from ADR-0012
 * (TA-11). The `x-client-` suffix is bounded so a long run of them stays linear; a suffix longer
 * than 64 characters is not a real header name.
 */
const HEADER_NAMES = [
  'cookie',
  'authorization',
  'forwarded',
  'x-forwarded-for',
  'x-real-ip',
  'x-internal-secret',
  'x-client-[a-z0-9-]{1,64}',
].join('|');

/**
 * A sensitive header name, an optional closing quote (also escaped, `\"`, as in JSON inside a
 * string) and its separator: `:`, `=`, the `=>` of `util.inspect(new Map())` or the `, value:` of a
 * `{ name, value }` pair, on one line or several (TA-11). The value that follows is consumed by
 * `redactHeaderValues()`, which reads its structure instead of stopping at the first quote or `]`
 * (PR #2 review). Group 1 is the closing quote, if any.
 */
const OPTIONAL_QUOTE = `(?:\\\\?["'\`])?`;
const VALUE_KEY = `,\\s*${OPTIONAL_QUOTE}value${OPTIONAL_QUOTE}\\s*[:=]`;
const SENSITIVE_HEADER = new RegExp(
  `\\b(?:${HEADER_NAMES})(\\\\?["'\`])?\\s*(?:=>|[:=]|${VALUE_KEY})[ \\t]*`,
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

const CLOSERS: Record<string, string> = { '[': ']', '{': '}' };

/**
 * End of an array or object that opens at `start` (after its closing bracket); strings may hold
 * brackets.
 */
function bracketEnd(text: string, start: number): number {
  const open = text[start];
  const close = CLOSERS[open ?? ''];
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    const ch = text[i] ?? '';
    if (QUOTES.has(ch)) {
      const end = stringEnd(text, i);
      if (end < 0) return text.length;
      i = end - 1;
    } else if (ch === open) depth++;
    else if (ch === close && --depth === 0) return i + 1;
  }
  return text.length;
}

/**
 * Where the value of a header starts: right after the separator, or on the next non-blank line
 * when the rest of this one is blank (`util.inspect` breaks a long entry after `=>`). The skipped
 * whitespace is kept.
 */
function valueStart(text: string, start: number): number {
  let i = start;
  while (text[i] === ' ' || text[i] === '\t') i++;
  if (text[i] !== '\n' && text[i] !== '\r') return start;
  while (i < text.length && /\s/.test(text[i] ?? '')) i++;
  return i;
}

/** End of the line that holds `i` (before its line break), or the end of the input. */
function lineEnd(text: string, i: number): number {
  while (i < text.length && text[i] !== '\n' && text[i] !== '\r') i++;
  return i;
}

/** What may follow a whole value on its line: the end of an entry, a structure or the line. */
const AFTER_VALUE = new Set([',', '}', ']', ')', '\n', '\r', '']);

/**
 * True when a value that closed at `end` also ends there: the next non-blank character of the
 * line closes an entry or a structure. Anything else (`{"a":1}; sid=…`, `"x".SECRET`) means the
 * value goes on, and `;` is not an end because it separates cookies (PR #51 review).
 */
function endsAt(text: string, end: number): boolean {
  let i = end;
  while (text[i] === ' ' || text[i] === '\t') i++;
  return AFTER_VALUE.has(text[i] ?? '');
}

/**
 * Redacts every sensitive header value in one left-to-right pass (linear: each value is read
 * once, and the text between values is scanned once for brackets). A value ends where its string,
 * array or object closes only inside a structure: the header name is quoted (JSON, `Map`) or an
 * unclosed `{` or `[` comes before it on its line (`util.inspect`), and the next character closes
 * an entry or a structure. Anything else goes to the end of the line, so a raw header line
 * (`Cookie: {}}; __Host-sid=…`) is never cut short (PR #51 review). The text before the name is
 * assumed to come from the formatter (`util.inspect`, JSON), not from the request. The quotes of a whole string
 * are kept. A string, an array or an object that never closes is redacted to the end of the
 * input: when in doubt, redact more.
 */
function redactHeaderValues(text: string): string {
  let out = '';
  let last = 0;
  let scanned = 0;
  let depth = 0;
  SENSITIVE_HEADER.lastIndex = 0;
  for (let match; (match = SENSITIVE_HEADER.exec(text)) !== null;) {
    for (; scanned < match.index; scanned++) {
      const ch = text[scanned];
      if (ch === '\n' || ch === '\r') depth = 0;
      else if (ch === '{' || ch === '[') depth++;
      else if ((ch === '}' || ch === ']') && depth > 0) depth--;
    }
    const structured = match[1] !== undefined || depth > 0;
    const start = valueStart(text, match.index + match[0].length);
    const first = text[start] ?? '';
    let end: number;
    let replacement = REDACTED;
    if (first in CLOSERS) {
      end = bracketEnd(text, start);
    } else if (QUOTES.has(first)) {
      const close = stringEnd(text, start);
      end = close < 0 ? text.length : close;
      replacement = close < 0 ? `${first}${REDACTED}` : `${first}${REDACTED}${first}`;
    } else {
      end = lineEnd(text, start);
    }
    if (!structured || !endsAt(text, end)) {
      end = lineEnd(text, end);
      if (QUOTES.has(first)) replacement = `${first}${REDACTED}`;
    }
    out += text.slice(last, start) + replacement;
    last = end;
    scanned = Math.max(scanned, end);
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

/**
 * A dotted IPv4 that ends an IPv6 (`::ffff:203.0.113.77`). A trailing `.` is allowed when no digit
 * follows it, so the full stop of a sentence does not leave `.0.113.77` behind.
 */
const IPV4_TAIL = /(?:\d{1,3}\.){3}\d{1,3}(?!\d|\.\d)/y;
const ZONE = /%[0-9a-z._~-]+/iy;
const HEX_DIGIT = /[0-9a-f]/i;
/** A character that glues a label to the hex group after it (`id:` → `d` is not a group). */
const GLUE = /[g-z_]/i;
/** A group of hex letters only (`d` of `id`, `de` of `node`): the tail of a word. */
const HEX_LETTERS = /^[a-f]{1,4}(?![0-9a-f])/i;

const isHex = (ch: string | undefined): boolean => ch !== undefined && HEX_DIGIT.test(ch);

/** Length of the hex group at `i` (1 to 4), or 0 if there is none or it is longer. */
function groupLength(text: string, i: number): number {
  let n = 0;
  while (n < 5 && isHex(text[i + n])) n++;
  return n <= 4 ? n : 0;
}

/**
 * End of the longest IPv6 that starts at `i` (full, or compressed with `::`, optionally ending in
 * a dotted IPv4 and a zone), or -1. Reads at most 8 groups, so the cost per start is bounded.
 */
function ipv6End(text: string, i: number): number {
  let groups = 0;
  let compressed = text.startsWith('::', i);
  let best = -1;
  if (compressed) i += 2;
  for (;;) {
    // An IPv4 counts as two groups and only goes last.
    IPV4_TAIL.lastIndex = i;
    const ipv4 = IPV4_TAIL.exec(text);
    if (ipv4 && (compressed ? groups + 2 <= 7 : groups + 2 === 8)) {
      best = i + ipv4[0].length;
      break;
    }
    const length = groupLength(text, i);
    if (length === 0) break;
    groups++;
    i += length;
    if (compressed ? groups > 7 : groups > 8) break;
    if (compressed || groups === 8) best = i;
    if (groups === 8) break;
    if (!compressed && text.startsWith('::', i)) {
      compressed = true;
      i += 2;
      best = i;
    } else if (text[i] === ':' && isHex(text[i + 1])) i++;
    else break;
  }
  if (best < 0) return -1;
  ZONE.lastIndex = best;
  return ZONE.test(text) ? ZONE.lastIndex : best;
}

/**
 * Replaces every IPv6 with REDACTED_IP. Each hex group and each `::` is tried as a start, and the
 * union of every address found is redacted, so a key that looks like hex (`face:::2001:db8:1`,
 * `deadbeef:2001:db8::1`) never hides the address after it: when in doubt, redact more. A group
 * of hex letters glued to a label and followed by a single `:` (`id:2001:db8::1`) is left out
 * only when the rest is an address by itself; a group with digits (`x2001:db8::1`) or before `::`
 * (`client_fdab::1`) never is. A bare `::` is not an address.
 * Linear: every start reads at most 8 groups.
 */
function redactIpv6(text: string): string {
  let out = '';
  let last = 0;
  let spanStart = -1;
  let spanEnd = -1;
  const flush = () => {
    if (spanStart < 0) return;
    out += text.slice(last, spanStart) + REDACTED_IP;
    last = spanEnd;
  };
  for (let i = 0; i < text.length; i++) {
    const groupStart = isHex(text[i]) && !isHex(text[i - 1]);
    if (!groupStart && !text.startsWith('::', i)) continue;
    const end = ipv6End(text, i);
    if (end < 0 || text.slice(i, end) === '::') continue;
    if (groupStart && GLUE.test(text[i - 1] ?? '') && HEX_LETTERS.test(text.slice(i, i + 5))) {
      const next = i + groupLength(text, i);
      if (text[next] === ':' && text[next + 1] !== ':' && ipv6End(text, next + 1) >= end) continue;
    }
    if (i > spanEnd) {
      flush();
      spanStart = i;
    }
    spanEnd = Math.max(spanEnd, end);
  }
  flush();
  return out + text.slice(last);
}

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
  const redacted = redactHeaderValues(truncate(text))
    .replace(URL_CREDENTIALS, `$1${REDACTED}@`)
    .replace(QUERY, '')
    .replace(FRAGMENT, '');
  return redactIpv6(redacted).replace(IPV4, REDACTED_IP);
}
