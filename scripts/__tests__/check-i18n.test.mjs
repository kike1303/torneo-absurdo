import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkI18n } from '../check-i18n.mjs';

const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-i18n/', import.meta.url));
const run = (name) => checkI18n({ webDir: path.join(FIXTURES, name) });

describe('check-i18n (RF-A15)', () => {
  it('passes on a valid project', () => {
    expect(run('valid')).toEqual([]);
  });

  it.each([
    ['missing-key', /messages\/es\.json:1: missing key `bye`/],
    ['empty-message', /messages\/es\.json:\d+: empty message `bye`/],
    ['literal-text', /src\/routes\/\+page\.svelte:1: literal text `Hello world`/],
    ['literal-aria-label', /ui\/x\.svelte:1: literal `aria-label` `Close`/],
    ['literal-title', /\+page\.svelte:1: literal `title`/],
    ['literal-alt', /\+page\.svelte:1: literal `alt`/],
    ['literal-placeholder', /\+page\.svelte:1: literal `placeholder`/],
    ['remote-module', /settings\.json:\d+: remote module/],
    ['remote-module-uppercase', /settings\.json:\d+: remote module/],
  ])('fails on %s', (name, pattern) => {
    const problems = run(name);
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n'),
    ).toBe(true);
  });

  it('exempts literal text under src/routes/dev/**', () => {
    expect(run('dev-exempt')).toEqual([]);
  });
});
