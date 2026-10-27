import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint } from 'eslint';
import { describe, expect, it } from 'vitest';
import { createConfig } from '../eslint.config.js';

// RF-B1 and plan-b decisions 2 and 13.
const ROOT = fileURLToPath(new URL('..', import.meta.url));
const eslint = new ESLint({ cwd: ROOT, overrideConfigFile: true, overrideConfig: createConfig() });

async function rules(file: string, code: string): Promise<string[]> {
  const [result] = await eslint.lintText(code, { filePath: path.join(ROOT, file) });
  const fatal = result?.messages.find((m) => m.fatal);
  if (fatal) throw new Error(`parse error in ${file}: ${fatal.message}`);
  return (result?.messages ?? []).map((m) => m.ruleId ?? 'unknown');
}

const GLOBALS = 'no-restricted-globals';
const PROPS = 'no-restricted-properties';
const IMPORTS = '@typescript-eslint/no-restricted-imports';

describe('the converter forbids clocks, randomness and foreign imports', () => {
  it.each([
    ['Math.random() in src', 'src/x.ts', 'export const r = Math.random();\n', PROPS],
    ['Date.now() in src', 'src/x.ts', 'export const d = Date.now();\n', GLOBALS],
    ['performance.now() in src', 'src/x.ts', 'export const p = performance.now();\n', GLOBALS],
    [
      'performance.now() in a test',
      'test/x.test.ts',
      'export const p = performance.now();\n',
      GLOBALS,
    ],
    [
      'node:crypto',
      'src/x.ts',
      "import { randomBytes } from 'node:crypto';\nexport const r = randomBytes;\n",
      IMPORTS,
    ],
    ["import 'pixi.js'", 'src/x.ts', "import 'pixi.js';\n", IMPORTS],
    [
      "import '@torneo/renderer'",
      'src/x.ts',
      "import { createRenderer } from '@torneo/renderer';\nexport const c = createRenderer;\n",
      IMPORTS,
    ],
    [
      '@torneo/renderer/timeline',
      'src/x.ts',
      "import { validateTimeline } from '@torneo/renderer/timeline';\nexport const v = validateTimeline;\n",
      IMPORTS,
    ],
    ['svelte', 'src/x.ts', "import 'svelte';\n", IMPORTS],
    [
      '@torneo/protocol',
      'src/x.ts',
      "import type { X } from '@torneo/protocol';\nexport type Y = X;\n",
      IMPORTS,
    ],
    [
      'renderer source by relative path',
      'src/x.ts',
      "import '../../../packages/renderer/src/index.ts';\n",
      IMPORTS,
    ],
  ])('%s fails', async (_name, file, code, rule) => {
    expect(await rules(file, code)).toContain(rule);
  });

  it.each([
    [
      'performance.now() in test/support/elapsed.ts',
      'test/support/elapsed.ts',
      'export const p = performance.now();\n',
    ],
    ['Date in src/cli.ts', 'src/cli.ts', 'export const d = Date.now();\n'],
    [
      '@torneo/renderer/manifest',
      'src/x.ts',
      "import { validateManifest } from '@torneo/renderer/manifest';\nexport const v = validateManifest;\n",
    ],
  ])('%s passes', async (_name, file, code) => {
    expect(await rules(file, code)).toEqual([]);
  });

  it('elapsed.ts may use performance, but not Date', async () => {
    expect(await rules('test/support/elapsed.ts', 'export const d = Date.now();\n')).toContain(
      GLOBALS,
    );
  });
});

describe('RF-B6 and plan-b decision 3', () => {
  const files = (dir: string): string[] =>
    readdirSync(path.join(ROOT, dir), { withFileTypes: true }).flatMap((e) =>
      e.isDirectory() ? files(path.join(dir, e.name)) : [path.join(dir, e.name)],
    );

  it('no test reads from the local content folder', () => {
    const needle = ['content', '_local'].join('/');
    for (const file of files('test')) {
      expect(readFileSync(path.join(ROOT, file), 'utf8'), file).not.toContain(needle);
    }
  });

  it('only png.ts and cli.ts have a catch in src/', () => {
    const withCatch = files('src').filter((file) =>
      /\bcatch\b/.test(
        readFileSync(path.join(ROOT, file), 'utf8').replace(/\/\/.*$|\/\*[\s\S]*?\*\//gm, ''),
      ),
    );
    expect(withCatch.map((f) => path.basename(f)).sort()).toEqual(['cli.ts', 'png.ts']);
  });
});
