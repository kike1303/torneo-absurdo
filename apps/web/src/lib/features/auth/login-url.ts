import { isCallbackErrorCode } from './codes.js';
import { isSafeNext } from './next.js';

export interface CanonicalLoginQuery {
  /** Whether the query that arrived differs from the canonical one (the `load` answers 303). */
  changed: boolean;
  /** `''` or `?…`, built only from fixed names and validated values. */
  search: string;
}

/**
 * The canonical query of `/login` (008 RF-D6, plan-d decision 7): `error` stays only if it is a
 * `CallbackErrorCode` and becomes `error=` (generic text) otherwise; `next` stays only if it is
 * safe and starts with `/<locale>/`; any other parameter, or a repeated one, is dropped. So no
 * component (the locale switcher copies `search` to its links) can ever reflect an unknown value.
 * The raw text is compared, so even a trailing `&` or another encoding gets the canonical form.
 */
export function canonicalLoginQuery(rawSearch: string): CanonicalLoginQuery {
  const raw = rawSearch.startsWith('?') ? rawSearch.slice(1) : rawSearch;
  const params = new URLSearchParams(raw);
  const out = new URLSearchParams();
  const errors = params.getAll('error');
  if (errors.length > 0) {
    const [error] = errors;
    out.set('error', errors.length === 1 && isCallbackErrorCode(error) ? error : '');
  }
  const nexts = params.getAll('next');
  if (nexts.length === 1 && isSafeNext(nexts[0])) out.set('next', nexts[0]);
  const canonical = out.toString();
  const search = canonical === '' ? '' : `?${canonical}`;
  return { changed: raw !== canonical, search };
}

/**
 * The query the locale redirect of the unprefixed `/login` carries (the server's callback
 * redirects there, QA of phase D): `?error=<code>` only if there is exactly one `error` and it
 * is a `CallbackErrorCode`, and `''` otherwise. Nothing else (`next`, unknown parameters, an
 * unknown `error`) is ever copied into the `Location`.
 */
export function loginRedirectSearch(rawSearch: string): string {
  const errors = new URLSearchParams(
    rawSearch.startsWith('?') ? rawSearch.slice(1) : rawSearch,
  ).getAll('error');
  const [error] = errors;
  return errors.length === 1 && isCallbackErrorCode(error) ? `?error=${error}` : '';
}
