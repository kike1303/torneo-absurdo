import { describe, expect, it } from 'vitest';
import { NEXT_CASES } from '../../../../tests/support/next-cases.js';
import { canonicalLoginQuery } from './login-url.js';

// 008 RF-D6 and the T4 case of the 2026-10-28 amendment (plan-d decision 7): the query of
// /login keeps only a known `error` and a safe `next`, so nothing unknown is ever reflected.
const CANARY = 'canary-login-9c1b';

const q = (search: string) => canonicalLoginQuery(search);

describe('canonicalLoginQuery', () => {
  it('a clean query stays as it is', () => {
    expect(q('')).toEqual({ changed: false, search: '' });
    expect(q('error=auth.banned')).toEqual({ changed: false, search: '?error=auth.banned' });
    expect(q('next=%2Fes%2Faccount')).toEqual({ changed: false, search: '?next=%2Fes%2Faccount' });
    expect(q('error=auth.try_again&next=%2Fen%2F')).toEqual({
      changed: false,
      search: '?error=auth.try_again&next=%2Fen%2F',
    });
    // `error=` (empty) is the canonical form of an unknown error.
    expect(q('error=')).toEqual({ changed: false, search: '?error=' });
    expect(q('?error=auth.banned')).toEqual({ changed: false, search: '?error=auth.banned' });
  });

  it.each([
    [`error=${CANARY}`, '?error='],
    ['error=%3Cscript%3Ealert(1)%3C%2Fscript%3E', '?error='],
    ['error=__proto__', '?error='],
    ['error=auth.csrf_origin', '?error='],
    ['error=%0d%0aSet-Cookie:x', '?error='],
    [`error=auth.banned&error=${CANARY}`, '?error='],
    [`x=${CANARY}`, ''],
    [`error=auth.banned&utm=${CANARY}`, '?error=auth.banned'],
    [`next=${encodeURIComponent('//evil.example')}`, ''],
    [`next=/es/a&next=/es/b`, ''],
    [`next=%2Fes%2Fa&error=auth.banned`, '?error=auth.banned&next=%2Fes%2Fa'],
    ['error=auth.banned&', '?error=auth.banned'],
    ['next=/es/account', '?next=%2Fes%2Faccount'],
  ])('%s → %j, without the unknown value', (search, expected) => {
    const result = q(search);
    expect(result).toEqual({ changed: true, search: expected });
    expect(result.search).not.toContain(CANARY);
    expect(result.search).not.toContain('script');
    expect(result.search.toLowerCase()).not.toContain('%0d');
  });

  it.each(NEXT_CASES)('next %j → kept only if safe (%j)', (next, expected) => {
    if (typeof next !== 'string') return;
    const { search } = q(new URLSearchParams({ next }).toString());
    expect(search).toBe(expected ? `?next=${encodeURIComponent(expected)}` : '');
  });
});
