// ESLint flat config for @torneo/sprite-importer (007 RF-B2, plan-b decision 10). Same structure
// as tools/sff-converter.
//
// In a flat config the last matching object wins for a rule, so every file group below gets the
// full, combined list of restrictions that applies to it.
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig, globalIgnores } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';

const CLOCK_MESSAGE =
  'The importer is deterministic: no clock nor randomness outside src/cli.ts (007 RF-B2).';
const IMPORT_MESSAGE =
  'The importer only uses @torneo/renderer/manifest and /guide (and /scene in tests): no Pixi, no framework, no protocol (007 plan-b decision 10).';

const CLOCK_NAMES = ['Date', 'performance', 'crypto'];

/** @type {import('eslint').Linter.RuleEntry} */
const restrictedGlobals = [
  'error',
  ...CLOCK_NAMES.map((name) => ({ name, message: CLOCK_MESSAGE })),
];

/** @type {import('eslint').Linter.RuleEntry} */
const restrictedSyntax = [
  'error',
  {
    selector: `MemberExpression[property.name=/^(${CLOCK_NAMES.join('|')})$/]`,
    message: CLOCK_MESSAGE,
  },
  {
    selector:
      'MemberExpression[property.name=/^(random|getRandomValues|randomUUID)$/][object.name=/^(Math|crypto)$/]',
    message: CLOCK_MESSAGE,
  },
];

/** @type {import('eslint').Linter.RuleEntry} */
const restrictedProperties = [
  'error',
  { object: 'Math', property: 'random', message: CLOCK_MESSAGE },
  { object: 'crypto', property: 'getRandomValues', message: CLOCK_MESSAGE },
  { object: 'crypto', property: 'randomUUID', message: CLOCK_MESSAGE },
];

/**
 * @param {{ clock: boolean, subpaths: readonly string[] }} options
 * @returns {import('eslint').Linter.RuleEntry}
 */
function restrictedImports({ clock, subpaths }) {
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
        { regex: `^@torneo/renderer/(?!(?:${subpaths.join('|')})$)`, message: IMPORT_MESSAGE },
        {
          group: ['pixi.js/*', 'svelte/*', '@sveltejs/*', '@torneo/protocol/*'],
          message: IMPORT_MESSAGE,
        },
        { group: ['**/packages/**', '**/apps/**', '**/renderer/src/**'], message: IMPORT_MESSAGE },
      ],
    },
  ];
}

const SUBPATHS = ['manifest', 'guide'];

export function createConfig() {
  return defineConfig([
    globalIgnores(['node_modules/', 'fixtures/']),
    js.configs.recommended,
    ...ts.configs.recommended,
    prettier,
    {
      languageOptions: { globals: { ...globals.node } },
      rules: {
        '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
        'no-restricted-globals': restrictedGlobals,
        'no-restricted-properties': restrictedProperties,
        'no-restricted-syntax': restrictedSyntax,
        '@typescript-eslint/no-restricted-imports': restrictedImports({
          clock: true,
          subpaths: SUBPATHS,
        }),
      },
    },
    {
      // The replay test plays ref_a with prepareScene and sceneAt (plan-b decision 18).
      files: ['test/**'],
      rules: {
        '@typescript-eslint/no-restricted-imports': restrictedImports({
          clock: true,
          subpaths: [...SUBPATHS, 'scene'],
        }),
      },
    },
    {
      // The CLI may read the clock, although today it does not.
      files: ['src/cli.ts'],
      rules: {
        'no-restricted-globals': 'off',
        'no-restricted-properties': 'off',
        'no-restricted-syntax': 'off',
        '@typescript-eslint/no-restricted-imports': restrictedImports({
          clock: false,
          subpaths: SUBPATHS,
        }),
      },
    },
  ]);
}

export default createConfig();
