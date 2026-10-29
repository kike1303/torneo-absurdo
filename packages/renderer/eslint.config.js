// ESLint flat config for @torneo/renderer (RF-A3, plan-a1 decision 10).
//
// In a flat config the last matching object wins for a rule, so every file group below gets the
// full, combined list of restrictions that applies to it.
//
// `no-restricted-imports` reads `group` patterns with .gitignore syntax, where a line starting
// with `#` is a comment, so the `#lib` patterns are anchored regexes (as in apps/web).
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig, globalIgnores } from 'eslint/config';
import globals from 'globals';
import ts from 'typescript-eslint';

const CLOCK_MESSAGE =
  'Time comes from the injected clock (RF-A11) and randomness from src/fx/prng.ts (RF-A10).';
const DOM_MESSAGE =
  'The pure core (manifest, timeline, scene, fx, player, stats, guide) has no DOM (RF-A3).';
const FRAMEWORK_MESSAGE =
  'The renderer is framework agnostic and decoupled from the protocol (RF-A3).';
const PIXI_MESSAGE = 'Only src/stage/ imports pixi.js; the pure core never does (RF-A3).';
const PIXI_WORKER_MESSAGE =
  'No compressed textures, GIFs nor workers: they need loaders the CSP blocks (ADR-0012 B4.2, plan-a2 decision 11).';
/** Pixi submodules that pull workers or loaders with workers; forbidden in the whole package. */
const PIXI_FORBIDDEN = ['ktx2', 'basis', 'ktx', 'dds', 'webworker', 'gif'].map(
  (name) => `pixi.js/${name}`,
);

/** Pure directories: no Pixi and no DOM. */
const PURE = [
  'src/manifest/**/*.ts',
  'src/timeline/**/*.ts',
  'src/scene/**/*.ts',
  'src/fx/**/*.ts',
  'src/player/**/*.ts',
  'src/stats/**/*.ts',
  'src/guide/**/*.ts',
];

const clockGlobals = ['Date', 'performance', 'crypto'].map((name) => ({
  name,
  message: CLOCK_MESSAGE,
}));
const domGlobals = [
  'window',
  'document',
  'navigator',
  'HTMLElement',
  'requestAnimationFrame',
  'fetch',
].map((name) => ({ name, message: DOM_MESSAGE }));

/** @type {import('eslint').Linter.RuleEntry} */
const restrictedSyntax = [
  'error',
  {
    selector: 'MemberExpression[property.name=/^(Date|performance|crypto)$/]',
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

/** @param {boolean} pure @returns {import('eslint').Linter.RuleEntry} */
function restrictedImports(pure) {
  return [
    'error',
    {
      paths: [
        { name: 'svelte', message: FRAMEWORK_MESSAGE },
        { name: '@torneo/protocol', message: FRAMEWORK_MESSAGE },
        ...(pure ? [{ name: 'pixi.js', message: PIXI_MESSAGE }] : []),
        ...PIXI_FORBIDDEN.map((name) => ({ name, message: PIXI_WORKER_MESSAGE })),
      ],
      patterns: [
        { group: ['svelte/*', '@sveltejs/*', '@torneo/protocol/*'], message: FRAMEWORK_MESSAGE },
        { regex: '^#lib(?:/|$)', message: FRAMEWORK_MESSAGE },
        // A relative path from packages/renderer never names `packages/`, so match the folder name.
        { group: ['**/protocol-ts/**', '**/apps/web/**'], message: FRAMEWORK_MESSAGE },
        ...(pure ? [{ group: ['pixi.js/*'], message: PIXI_MESSAGE }] : []),
        {
          group: PIXI_FORBIDDEN.flatMap((name) => [
            `${name}/**`,
            `**/pixi.js/lib/${name.slice(8)}/**`,
          ]),
          message: PIXI_WORKER_MESSAGE,
        },
      ],
    },
  ];
}

export function createConfig() {
  return defineConfig([
    globalIgnores(['node_modules/']),
    js.configs.recommended,
    ...ts.configs.recommended,
    prettier,
    {
      languageOptions: { globals: { ...globals.browser, ...globals.node } },
      rules: {
        '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
        'no-restricted-globals': ['error', ...clockGlobals],
        'no-restricted-properties': restrictedProperties,
        'no-restricted-syntax': restrictedSyntax,
        '@typescript-eslint/no-restricted-imports': restrictedImports(false),
      },
    },
    {
      files: PURE,
      rules: {
        'no-restricted-globals': ['error', ...clockGlobals, ...domGlobals],
        '@typescript-eslint/no-restricted-imports': restrictedImports(true),
      },
    },
  ]);
}

export default createConfig();
