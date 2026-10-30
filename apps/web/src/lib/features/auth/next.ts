import { localeFromPathname, type Locale } from '#lib/i18n/locale.js';

/** Far longer than any of our routes, short enough to keep a `Location` sane. */
const MAX_NEXT_LENGTH = 512;

/**
 * Whether `value` is a destination of ours (008 plan-d decision 9): the rule of ADR-0008 D2 (a
 * local path, not `//` nor `/\`, without C0, DEL or C1 control characters) **and** a locale
 * prefix `/<locale>/`, the same as the server's `next` plus the prefix.
 */
export function isSafeNext(value: unknown): value is string {
  if (typeof value !== 'string' || value.length > MAX_NEXT_LENGTH) return false;
  if (!value.startsWith('/') || value[1] === '/' || value[1] === '\\') return false;
  // eslint-disable-next-line no-control-regex -- the control characters are what this rejects
  if (/[\u0000-\u001f\u007f-\u009f]/.test(value)) return false;
  const locale = localeFromPathname(value);
  return locale !== undefined && value.startsWith(`/${locale}/`);
}

/** `value` if it passes {@link isSafeNext}, otherwise `/<fallback>/`. */
export function safeNext(value: string | null | undefined, fallback: Locale): string {
  return isSafeNext(value) ? value : `/${fallback}/`;
}
