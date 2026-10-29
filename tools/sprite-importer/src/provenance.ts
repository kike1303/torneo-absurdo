// `provenance.json`, `torneo.provenance` v1 (007 RF-B4): exact keys and no free text. Who the
// author is and the proof of the rights live in private/provenance/<id>.md, never here.
import { PROVENANCE_CODES as P } from './codes.ts';
import type { IssueSink } from './report.ts';

export const PROVENANCE_FORMAT = 'torneo.provenance';
export const PROVENANCE_FORMAT_VERSION = 1;

const KEYS = [
  'format',
  'format_version',
  'set',
  'author',
  'origin',
  'date',
  'ai_assisted',
  'license',
] as const;
const AUTHOR = /^[a-z0-9]+(-[a-z0-9]+)*$/;
const AUTHOR_MIN = 2;
const AUTHOR_MAX = 32;
const ORIGINS: readonly unknown[] = ['original', 'commission', 'generated'];
const DATE = /^([0-9]{4})-([0-9]{2})-([0-9]{2})$/;

/** A valid calendar date, with integer arithmetic (no `Date`, which the lint rules forbid). */
export function isCalendarDate(value: string): boolean {
  const match = DATE.exec(value);
  if (match === null) return false;
  const year = Number(match[1]);
  const month = Number(match[2]);
  const day = Number(match[3]);
  if (month < 1 || month > 12 || day < 1) return false;
  const leap = (year % 4 === 0 && year % 100 !== 0) || year % 400 === 0;
  const days = [31, leap ? 29 : 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31][month - 1] ?? 0;
  return day <= days;
}

function isObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const proto: unknown = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}

/** Checks the parsed JSON of `file` (relative to the root) for the set in folder `id`. */
export function checkProvenance(sink: IssueSink, file: string, json: unknown, id: string): void {
  if (!isObject(json)) {
    sink.add(P.invalidValue, file, '');
    return;
  }
  const keys: readonly string[] = KEYS;
  for (const key of Object.keys(json)) {
    if (!keys.includes(key)) sink.add(P.unknownField, file, `/${key}`);
  }
  for (const key of KEYS) {
    if (!Object.hasOwn(json, key)) sink.add(P.missingField, file, `/${key}`);
  }
  const has = (key: string) => Object.hasOwn(json, key);
  const value = (key: string): unknown => (has(key) ? json[key] : undefined);
  const invalid = (key: string, ok: boolean) => {
    if (has(key) && !ok) sink.add(P.invalidValue, file, `/${key}`);
  };
  invalid('format', value('format') === PROVENANCE_FORMAT);
  invalid('format_version', value('format_version') === PROVENANCE_FORMAT_VERSION);
  const set = value('set');
  if (has('set') && set !== id) sink.add(P.setMismatch, file, '/set');
  const author = value('author');
  invalid(
    'author',
    typeof author === 'string' &&
      author.length >= AUTHOR_MIN &&
      author.length <= AUTHOR_MAX &&
      AUTHOR.test(author),
  );
  invalid('origin', ORIGINS.includes(value('origin')));
  const date = value('date');
  invalid('date', typeof date === 'string' && isCalendarDate(date));
  invalid('ai_assisted', typeof value('ai_assisted') === 'boolean');
  invalid('license', value('license') === 'content');
}
