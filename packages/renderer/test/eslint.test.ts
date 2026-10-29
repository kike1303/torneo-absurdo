import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint } from 'eslint';
import { describe, expect, it } from 'vitest';
import { createConfig } from '../eslint.config.js';

// RF-A3 and plan-a1 decision 10.
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
const SYNTAX = 'no-restricted-syntax';
const IMPORTS = '@typescript-eslint/no-restricted-imports';

type Case = [name: string, file: string, code: string, rule: string];

describe('the renderer forbids clocks, randomness and framework imports', () => {
  it.each<Case>([
    ['Math.random()', 'src/stage/x.ts', 'export const r = Math.random();\n', PROPS],
    ['Date.now()', 'src/scene/x.ts', 'export const d = Date.now();\n', GLOBALS],
    ['new Date()', 'src/index.ts', 'export const d = new Date();\n', GLOBALS],
    ['performance.now()', 'src/fx/x.ts', 'export const p = performance.now();\n', GLOBALS],
    [
      'globalThis.performance.now()',
      'src/stage/x.ts',
      'export const p = globalThis.performance.now();\n',
      SYNTAX,
    ],
    [
      'crypto.getRandomValues(x)',
      'src/stage/x.ts',
      'const x = new Uint32Array(1);\ncrypto.getRandomValues(x);\n',
      GLOBALS,
    ],
    ['Date in a test', 'test/x.test.ts', 'export const d = Date.now();\n', GLOBALS],
    ["import 'svelte'", 'src/stage/x.ts', "import 'svelte';\n", IMPORTS],
    [
      '@sveltejs/kit',
      'src/stage/x.ts',
      "import { error } from '@sveltejs/kit';\nexport const e = error;\n",
      IMPORTS,
    ],
    ['#lib', 'src/stage/x.ts', "import x from '#lib/utils.js';\nexport const y = x;\n", IMPORTS],
    [
      'import type from @torneo/protocol',
      'src/scene/x.ts',
      "import type { X } from '@torneo/protocol';\nexport type Y = X;\n",
      IMPORTS,
    ],
    [
      'protocol-ts by relative path',
      'src/stage/x.ts',
      "import type { X } from '../../../protocol-ts/src/index.ts';\nexport type Y = X;\n",
      IMPORTS,
    ],
    ["import 'pixi.js' in scene", 'src/scene/x.ts', "import 'pixi.js';\n", IMPORTS],
    ["import 'pixi.js' in fx", 'src/fx/x.ts', "import 'pixi.js';\n", IMPORTS],
    [
      'pixi.js subpath in manifest',
      'src/manifest/x.ts',
      "import 'pixi.js/unsafe-eval';\n",
      IMPORTS,
    ],
    ...['ktx2', 'basis', 'ktx', 'dds', 'webworker', 'gif'].map(
      (name) =>
        [
          `pixi.js/${name} in stage`,
          'src/stage/x.ts',
          `import 'pixi.js/${name}';\n`,
          IMPORTS,
        ] satisfies Case,
    ),
    ["import 'pixi.js' in player", 'src/player/x.ts', "import 'pixi.js';\n", IMPORTS],
    ["import 'pixi.js' in stats", 'src/stats/x.ts', "import 'pixi.js';\n", IMPORTS],
    ["import 'pixi.js' in guide", 'src/guide/x.ts', "import 'pixi.js';\n", IMPORTS],
    ['document in the guide', 'src/guide/x.ts', 'export const b = document.body;\n', GLOBALS],
    ['document in the player', 'src/player/x.ts', 'export const b = document.body;\n', GLOBALS],
    [
      'performance.now() in stats',
      'src/stats/x.ts',
      'export const p = performance.now();\n',
      GLOBALS,
    ],
    [
      'document in the pure core',
      'src/timeline/x.ts',
      'export const b = document.body;\n',
      GLOBALS,
    ],
  ])('%s fails', async (_name, file, code, rule) => {
    expect(await rules(file, code)).toContain(rule);
  });

  it.each([
    ["import 'pixi.js' in stage", 'src/stage/x.ts', "import 'pixi.js';\n"],
    ['document in stage', 'src/stage/x.ts', 'export const b = document.body;\n'],
    ['pixi.js/unsafe-eval in the entry', 'src/index.ts', "import 'pixi.js/unsafe-eval';\n"],
    [
      'the package PRNG',
      'src/fx/x.ts',
      "import { mulberry32 } from './prng.ts';\nexport const r = mulberry32(1)();\n",
    ],
  ])('%s passes', async (_name, file, code) => {
    expect(await rules(file, code)).toEqual([]);
  });

  it('document is caught only by the pure group', async () => {
    expect(await rules('src/stage/x.ts', 'export const b = document.body;\n')).toEqual([]);
    expect(await rules('src/scene/x.ts', 'export const b = document.body;\n')).toEqual([GLOBALS]);
  });
});
