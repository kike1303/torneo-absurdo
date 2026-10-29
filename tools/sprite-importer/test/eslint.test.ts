import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint } from 'eslint';
import { describe, expect, it } from 'vitest';
import { createConfig } from '../eslint.config.js';

// 007 plan-b decision 10: the three import boundaries, and no clock nor randomness.
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
const use = (spec: string) => `import * as m from '${spec}';\nexport const x = m;\n`;

describe('the importer forbids clocks, randomness and foreign imports', () => {
  it.each([
    ['Math.random() in src', 'src/x.ts', 'export const r = Math.random();\n', PROPS],
    ['Math.random() in scripts', 'scripts/x.ts', 'export const r = Math.random();\n', PROPS],
    ['Date.now() in src', 'src/x.ts', 'export const d = Date.now();\n', GLOBALS],
    ['new Date() in src', 'src/x.ts', 'export const d = new Date();\n', GLOBALS],
    ['node:crypto', 'src/x.ts', use('node:crypto'), IMPORTS],
    ['pixi.js', 'src/x.ts', use('pixi.js'), IMPORTS],
    ['@torneo/renderer', 'src/x.ts', use('@torneo/renderer'), IMPORTS],
    ['@torneo/renderer/timeline', 'src/x.ts', use('@torneo/renderer/timeline'), IMPORTS],
    ['@torneo/renderer/scene in src', 'src/x.ts', use('@torneo/renderer/scene'), IMPORTS],
    ['@torneo/renderer/scene in scripts', 'scripts/x.ts', use('@torneo/renderer/scene'), IMPORTS],
    [
      '@torneo/renderer/timeline in test',
      'test/x.test.ts',
      use('@torneo/renderer/timeline'),
      IMPORTS,
    ],
    [
      'renderer source by relative path',
      'test/x.test.ts',
      use('../../../packages/renderer/src/scene/index.ts'),
      IMPORTS,
    ],
  ])('%s fails', async (_name, file, code, rule) => {
    expect(await rules(file, code)).toContain(rule);
  });

  it.each([
    ['@torneo/renderer/manifest in src', 'src/x.ts', use('@torneo/renderer/manifest')],
    ['@torneo/renderer/guide in src', 'src/x.ts', use('@torneo/renderer/guide')],
    ['@torneo/renderer/guide in scripts', 'scripts/x.ts', use('@torneo/renderer/guide')],
    ['@torneo/renderer/scene in test', 'test/x.test.ts', use('@torneo/renderer/scene')],
    ['Date in src/cli.ts', 'src/cli.ts', 'export const d = Date.now();\n'],
  ])('%s passes', async (_name, file, code) => {
    expect(await rules(file, code)).toEqual([]);
  });
});
