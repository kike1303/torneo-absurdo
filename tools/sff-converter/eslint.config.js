// ESLint flat config for @torneo/sff-converter (002 RF-B1, plan-b decisions 2 and 13).
//
// In a flat config the last matching object wins for a rule, so every file group below gets the
// full, combined list of restrictions that applies to it.
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig, globalIgnores } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';

const CLOCK_MESSAGE =
  'The converter is deterministic: no clock nor randomness outside src/cli.ts (RF-B1).';
const ELAPSED_MESSAGE = 'Only test/support/elapsed.ts measures time (RF-B4).';
const IMPORT_MESSAGE =
  'The converter only uses @torneo/renderer/manifest: no Pixi, no framework, no protocol (RF-B1).';

const CLOCK_NAMES = ['Date', 'performance', 'crypto'];

/** @param {readonly string[]} allowed @returns {import('eslint').Linter.RuleEntry} */
function restrictedGlobals(allowed) {
  return [
    'error',
    ...CLOCK_NAMES.filter((name) => !allowed.includes(name)).map((name) => ({
      name,
      message: name === 'performance' ? ELAPSED_MESSAGE : CLOCK_MESSAGE,
    })),
  ];
}

/** @param {readonly string[]} allowed @returns {import('eslint').Linter.RuleEntry} */
function restrictedSyntax(allowed) {
  const names = CLOCK_NAMES.filter((name) => !allowed.includes(name));
  return [
    'error',
    ...(names.length > 0
      ? [
          {
            selector: `MemberExpression[property.name=/^(${names.join('|')})$/]`,
            message: CLOCK_MESSAGE,
          },
        ]
      : []),
    {
      selector:
        'MemberExpression[property.name=/^(random|getRandomValues|randomUUID)$/][object.name=/^(Math|crypto)$/]',
      message: CLOCK_MESSAGE,
    },
  ];
}

/** @type {import('eslint').Linter.RuleEntry} */
const restrictedProperties = [
  'error',
  { object: 'Math', property: 'random', message: CLOCK_MESSAGE },
  { object: 'crypto', property: 'getRandomValues', message: CLOCK_MESSAGE },
  { object: 'crypto', property: 'randomUUID', message: CLOCK_MESSAGE },
];

/** @param {boolean} clock @returns {import('eslint').Linter.RuleEntry} */
function restrictedImports(clock) {
  return [
    'error',
    {
      paths: [
        { name: '@torneo/renderer', message: IMPORT_MESSAGE },
        { name: 'pixi.js', message: IMPORT_MESSAGE },
        { name: 'svelte', message: IMPORT_MESSAGE },
        { name: '@torneo/protocol', message: IMPORT_MESSAGE },
        ...(clock
          ? [
              { name: 'node:crypto', message: CLOCK_MESSAGE },
              { name: 'crypto', message: CLOCK_MESSAGE },
            ]
          : []),
      ],
      patterns: [
        { regex: '^@torneo/renderer/(?!manifest$)', message: IMPORT_MESSAGE },
        {
          group: ['pixi.js/*', 'svelte/*', '@sveltejs/*', '@torneo/protocol/*'],
          message: IMPORT_MESSAGE,
        },
        { group: ['**/packages/**', '**/apps/**', '**/renderer/src/**'], message: IMPORT_MESSAGE },
      ],
    },
  ];
}

export function createConfig() {
  return defineConfig([
    globalIgnores(['node_modules/', 'input/', 'output/']),
    js.configs.recommended,
    ...ts.configs.recommended,
    prettier,
    {
      languageOptions: { globals: { ...globals.node } },
      rules: {
        '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
        'no-restricted-globals': restrictedGlobals([]),
        'no-restricted-properties': restrictedProperties,
        'no-restricted-syntax': restrictedSyntax([]),
        '@typescript-eslint/no-restricted-imports': restrictedImports(true),
      },
    },
    {
      // The CLI may read the clock (spec RF-B1), although today it does not.
      files: ['src/cli.ts'],
      rules: {
        'no-restricted-globals': 'off',
        'no-restricted-properties': 'off',
        'no-restricted-syntax': 'off',
        '@typescript-eslint/no-restricted-imports': restrictedImports(false),
      },
    },
    {
      // Only `performance`, to measure the "< 1 s per case" of RF-B4 (plan-b precision 4).
      files: ['test/support/elapsed.ts'],
      rules: {
        'no-restricted-globals': restrictedGlobals(['performance']),
        'no-restricted-syntax': restrictedSyntax(['performance']),
      },
    },
  ]);
}

export default createConfig();
