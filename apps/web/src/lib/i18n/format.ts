import type { Locale } from './locale.js';

// Shared Intl formatters (ADR-0007, ADR-0009 D8). Components never format on their own.

/** Fictional coins (ADR-0005): an integer with the locale's grouping, no currency symbol. */
export function formatCoins(amount: number, locale: Locale): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(amount);
}

/** An integer with the locale's grouping (damage numbers, frame counts). */
export function formatInteger(n: number, locale: Locale): string {
  return new Intl.NumberFormat(locale, { maximumFractionDigits: 0 }).format(n);
}

/** A number with exactly `digits` decimals (frame statistics). */
export function formatDecimal(n: number, locale: Locale, digits: number): string {
  return new Intl.NumberFormat(locale, {
    minimumFractionDigits: digits,
    maximumFractionDigits: digits,
  }).format(n);
}

export function formatDate(
  date: Date,
  locale: Locale,
  options: Intl.DateTimeFormatOptions = { dateStyle: 'medium', timeStyle: 'short' },
): string {
  return new Intl.DateTimeFormat(locale, options).format(date);
}

const UNITS: [Intl.RelativeTimeFormatUnit, number][] = [
  ['year', 365 * 24 * 3600],
  ['month', 30 * 24 * 3600],
  ['week', 7 * 24 * 3600],
  ['day', 24 * 3600],
  ['hour', 3600],
  ['minute', 60],
  ['second', 1],
];

/** "in 5 minutes", "hace 2 días": the largest unit that fits. `now` is injectable for tests. */
/**
 * A day and an hour with the name of its zone: in `timeZone`, or in the local one without it.
 * `dateStyle` and `timeStyle` do not admit `timeZoneName`, so it goes by its parts.
 */
export function formatDateWithZone(date: Date, locale: Locale, timeZone?: string): string {
  return formatDate(date, locale, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: 'numeric',
    minute: '2-digit',
    timeZoneName: 'short',
    ...(timeZone === undefined ? {} : { timeZone }),
  });
}

export function formatRelative(date: Date, locale: Locale, now: Date = new Date()): string {
  const seconds = Math.round((date.getTime() - now.getTime()) / 1000);
  const format = new Intl.RelativeTimeFormat(locale, { numeric: 'auto' });
  for (const [unit, size] of UNITS) {
    if (Math.abs(seconds) >= size || unit === 'second') {
      return format.format(Math.round(seconds / size), unit);
    }
  }
  return format.format(0, 'second');
}

/** A countdown: `m:ss`, and `h:mm:ss` from one hour on. Rounds up: the zero only shows at the zero. */
export function formatClock(ms: number, locale: Locale): string {
  const total = Math.ceil(Math.max(0, ms) / 1000);
  const digits = (n: number, min: number) =>
    new Intl.NumberFormat(locale, { minimumIntegerDigits: min, useGrouping: false }).format(n);
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const seconds = total % 60;
  return hours > 0
    ? `${digits(hours, 1)}:${digits(minutes, 2)}:${digits(seconds, 2)}`
    : `${digits(minutes, 1)}:${digits(seconds, 2)}`;
}
