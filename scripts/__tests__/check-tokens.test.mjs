import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { checkTokens } from '../check-tokens.mjs';

const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-tokens/', import.meta.url));
const load = (dir, name) => ({
  file: `fixtures/${dir}/${name}`,
  source: readFileSync(path.join(FIXTURES, dir, name), 'utf8'),
});

describe('check-tokens (ADR-0009 test 1)', () => {
  it.each(readdirSync(path.join(FIXTURES, 'fail')))('fails on %s, naming file and line', (name) => {
    const problems = checkTokens([load('fail', name)]);
    expect(problems.length).toBeGreaterThan(0);
    expect(problems[0]).toMatch(new RegExp(`^fixtures/fail/${name.replace('.', '\\.')}:\\d+: `));
  });

  it.each(readdirSync(path.join(FIXTURES, 'pass')))('passes on %s', (name) => {
    expect(checkTokens([load('pass', name)])).toEqual([]);
  });

  it('exempts tokens.css and theme-color.ts', () => {
    expect(
      checkTokens([
        {
          file: 'apps/web/src/lib/theme/tokens.css',
          source: ':root { --palette-x: oklch(1 0 0); }',
        },
        { file: 'apps/web/src/lib/theme/theme-color.ts', source: "export const c = '#ffffff';" },
      ]),
    ).toEqual([]);
  });
});
