import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { checkEngineSplit, checkRendererSplit, checkWasmFile } from '../check-renderer-split.mjs';

// 002 RF-A17 and plan-a2 decision 9.
const FIXTURES = fileURLToPath(new URL('../__fixtures__/check-renderer-split/', import.meta.url));
const read = (name, file) => JSON.parse(readFileSync(path.join(FIXTURES, name, file), 'utf8'));
const run = (name) =>
  checkRendererSplit(read(name, 'manifest.json'), read(name, 'chunk-modules.json'));

describe('check-renderer-split (002 RF-A17)', () => {
  it('passes when Pixi, Three and the renderer only load through import() from dev/arena', () => {
    const result = run('pass');
    expect(result.problems).toEqual([]);
    expect(result.deferredFiles).toEqual([
      '_app/immutable/chunks/pixi.js',
      '_app/immutable/chunks/renderer.js',
      '_app/immutable/chunks/renderer3d.js',
      '_app/immutable/chunks/three.js',
    ]);
  });

  it('fails when the root layout reaches Three statically (013)', () => {
    const manifest = read('pass', 'manifest.json');
    manifest['.svelte-kit/generated/build/client-optimized/nodes/0.js'].imports.push('_three.js');
    expect(checkRendererSplit(manifest, read('pass', 'chunk-modules.json')).problems).toEqual([
      expect.stringMatching(/nodes\/0\.js: reaches .*three\/build\/.* through static imports/),
    ]);
  });

  it('fails when the root layout reaches Pixi statically (through a shared chunk)', () => {
    expect(run('pixi-static-root').problems).toEqual([
      expect.stringMatching(/nodes\/0\.js: reaches .*pixi\.js\/lib\/.* through static imports/),
      // This build predates the 3D arena (013).
      expect.stringMatching(/nodes\/4\.js: does not reach three through dynamic imports/),
    ]);
  });

  it('fails when the dev/arena node holds the renderer statically', () => {
    expect(run('renderer-static-arena').problems).toEqual([
      expect.stringMatching(/nodes\/4\.js: reaches .* through static imports/),
      expect.stringMatching(/nodes\/4\.js: does not reach pixi\.js through dynamic imports/),
      expect.stringMatching(/nodes\/4\.js: does not reach three through dynamic imports/),
    ]);
  });

  it('fails without the dynamic import (never passes empty)', () => {
    expect(run('no-dynamic').problems).toEqual([
      expect.stringMatching(/nodes\/4\.js: does not reach pixi\.js through dynamic imports/),
      expect.stringMatching(/nodes\/4\.js: does not reach three through dynamic imports/),
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

// 011 RF5.
describe('checkEngineSplit', () => {
  const GLUE = 'src/lib/features/replay/engine-wasm/engine_wasm.js';
  const PAGE = 'src/routes/fights/[id=fight_id]/+page.svelte';
  const manifestWith = (node = {}, root = {}) => ({
    'nodes/0.js': { file: 'nodes/0.js', src: 'nodes/0.js', isEntry: true, ...root },
    'nodes/9.js': { file: 'nodes/9.js', src: 'nodes/9.js', isEntry: true, ...node },
    'loader.js': { file: 'loader.js', src: 'loader.js' },
    'glue.js': { file: 'glue.js', src: 'glue.js' },
  });
  const modules = {
    'nodes/0.js': ['src/routes/+layout.svelte'],
    'nodes/9.js': [PAGE],
    'loader.js': ['src/lib/features/replay/engine-loader.ts'],
    'glue.js': [GLUE],
  };

  it('passes when the fight page reaches the engine only through import()', () => {
    expect(checkEngineSplit(manifestWith({ dynamicImports: ['glue.js'] }), modules)).toEqual([]);
  });

  it('fails when another node reaches it statically', () => {
    const manifest = manifestWith({ dynamicImports: ['glue.js'] }, { imports: ['glue.js'] });
    expect(checkEngineSplit(manifest, modules)).toEqual([
      `nodes/0.js: reaches ${GLUE} through static imports`,
    ]);
  });

  it('fails when the fight page holds it statically', () => {
    expect(checkEngineSplit(manifestWith({ imports: ['glue.js'] }), modules)).toEqual([
      `nodes/9.js: reaches ${GLUE} through static imports`,
      'nodes/9.js: does not reach engine-wasm through dynamic imports',
    ]);
  });

  it('never passes empty: without a dynamic edge, or without the page', () => {
    expect(checkEngineSplit(manifestWith(), modules)).toEqual([
      'nodes/9.js: does not reach engine-wasm through dynamic imports',
    ]);
    expect(checkEngineSplit(manifestWith(), { ...modules, 'nodes/9.js': [] })).toEqual([
      'no route node holds src/routes/fights/[id=fight_id]/+page.svelte',
    ]);
  });
});

describe('checkWasmFile', () => {
  const BYTES = Buffer.from([0, 97, 115, 109, 1]);

  function build(files) {
    const dir = mkdtempSync(path.join(tmpdir(), 'check-wasm-'));
    for (const [name, content] of Object.entries(files)) {
      mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
      writeFileSync(path.join(dir, name), content);
    }
    return dir;
  }

  it.each([
    ['one file with the same bytes', { 'assets/a.wasm': BYTES, 'x.js': 'ok' }, []],
    ['none', { 'x.js': 'ok' }, ['expected exactly one .wasm in the client build, found 0']],
    [
      'two',
      { 'a.wasm': BYTES, 'b.wasm': BYTES },
      ['expected exactly one .wasm in the client build, found 2'],
    ],
    [
      'other bytes',
      { 'a.wasm': Buffer.from([0, 1]) },
      ['a.wasm: is not the committed engine_wasm_bg.wasm'],
    ],
    [
      'an inlined copy',
      { 'a.wasm': BYTES, 'x.js': 'const u = "data:application/wasm;base64,AA"' },
      ['x.js: carries a data:application/wasm'],
    ],
  ])('%s', (_name, files, problems) => {
    const dir = build(files);
    try {
      expect(checkWasmFile(dir, BYTES).problems).toEqual(problems);
    } finally {
      rmSync(dir, { recursive: true });
    }
  });
});
