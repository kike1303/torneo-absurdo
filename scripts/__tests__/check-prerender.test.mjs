import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkPrerender } from '../check-prerender.mjs';

const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-prerender/', import.meta.url));
const run = (name) => checkPrerender(path.join(FIXTURES, name, 'routes'));

describe('check-prerender (RF-A2)', () => {
  it('fails on prerender = true', () => {
    expect(run('true')).toEqual([
      expect.stringMatching(/routes\/\+page\.ts:1: `prerender = true`/),
    ]);
  });
  it("fails on prerender = 'auto'", () => {
    expect(run('auto')).toEqual([
      expect.stringMatching(/\+page\.server\.ts:1: `prerender = 'auto'`/),
    ]);
  });
  it.each(['false', 'absent'])('passes when %s', (name) => {
    expect(run(name)).toEqual([]);
  });
});
