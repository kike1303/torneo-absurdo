import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkRendererSplit } from '../check-renderer-split.mjs';

// 002 RF-A17 and plan-a2 decision 9.
const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-renderer-split/', import.meta.url));
const read = (name, file) => JSON.parse(readFileSync(path.join(FIXTURES, name, file), 'utf8'));
const run = (name) =>
  checkRendererSplit(read(name, 'manifest.json'), read(name, 'chunk-modules.json'));

describe('check-renderer-split (002 RF-A17)', () => {
  it('passes when Pixi and the renderer only load through import() from dev/arena', () => {
    const result = run('pass');
    expect(result.problems).toEqual([]);
    expect(result.deferredFiles).toEqual([
      '_app/immutable/chunks/pixi.js',
      '_app/immutable/chunks/renderer.js',
    ]);
  });

  it('fails when the root layout reaches Pixi statically (through a shared chunk)', () => {
    expect(run('pixi-static-root').problems).toEqual([
      expect.stringMatching(/nodes\/0\.js: reaches .*pixi\.js\/lib\/.* through static imports/),
    ]);
  });

  it('fails when the dev/arena node holds the renderer statically', () => {
    expect(run('renderer-static-arena').problems).toEqual([
      expect.stringMatching(/nodes\/4\.js: reaches .* through static imports/),
      expect.stringMatching(/nodes\/4\.js: does not reach pixi\.js through dynamic imports/),
    ]);
  });

  it('fails without the dynamic import (never passes empty)', () => {
    expect(run('no-dynamic').problems).toEqual([
      expect.stringMatching(/nodes\/4\.js: does not reach pixi\.js through dynamic imports/),
    ]);
  });

  it('fails when no node holds the dev/arena page', () => {
    const manifest = read('pass', 'manifest.json');
    const modules = read('pass', 'chunk-modules.json');
    modules['_app/immutable/nodes/4.js'] = ['other.svelte'];
    expect(checkRendererSplit(manifest, modules).problems).toEqual([
      'no route node holds src/routes/dev/arena/+page.svelte',
    ]);
  });

  it('fails on an empty manifest', () => {
    expect(checkRendererSplit({}, {}).problems).toContain(
      'manifest: no entries (did the format change?)',
    );
  });
});
