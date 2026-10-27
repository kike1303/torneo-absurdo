import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkNoLocalSprites } from '../check-no-local-sprites.mjs';

// 002 RF-B7 and plan-b decision 14.
const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-no-local-sprites/', import.meta.url));
const run = (name) => checkNoLocalSprites(path.join(FIXTURES, name));

describe('check-no-local-sprites (002 RF-B7)', () => {
  it('passes a build without the dev-only serving (a source map may quote it in sourcesContent)', () => {
    expect(run('pass')).toEqual({ files: 3, problems: [] });
  });

  it('fails on a /__local-sprites/ URL in any nested file', () => {
    expect(run('leak-url').problems).toEqual(['client/_app/chunk.js: contains __local-sprites']);
  });

  it('fails on the plugin name', () => {
    expect(run('leak-plugin').problems).toEqual([
      'server/index.js: contains torneo:local-sprites',
      'server/index.js.map: contains __local-sprites',
    ]);
  });
});
