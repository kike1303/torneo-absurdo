import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkKitConfig } from '../check-kit-config.mjs';

const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-kit-config/', import.meta.url));
const run = (name) => checkKitConfig(path.join(FIXTURES, name));

describe('check-kit-config (RF-A10, plan-a decision 17)', () => {
  it('passes on a valid config, even with the names in a comment', () => {
    expect(run('valid')).toEqual([]);
  });
  it.each([
    ['trusted-origins', /vite\.config\.ts:\d+: `trustedOrigins`/],
    ['check-origin', /vite\.config\.ts:\d+: `checkOrigin`/],
    ['paths-origin', /vite\.config\.ts:\d+: `origin` inside `paths`/],
    ['paths-origin-dotted', /vite\.config\.ts:\d+: `paths\.origin`/],
    ['svelte-config', /svelte\.config\.js:1: /],
  ])('fails on %s', (name, pattern) => {
    const problems = run(name);
    expect(
      problems.some((p) => pattern.test(p)),
      problems.join('\n'),
    ).toBe(true);
  });
  it('passes on the real apps/web', () => {
    expect(checkKitConfig(fileURLToPath(new URL('../../apps/web', import.meta.url)))).toEqual([]);
  });
});
