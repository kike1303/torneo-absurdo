// ESLint flat config for apps/web (RF-A14, ADR-0009 D4 with the Kit 3 `#lib` amendment).
//
// `no-restricted-imports` reads `group` patterns with .gitignore syntax, where a line starting
// with `#` is a comment: `#lib/features/**` would never match. Every `#lib` pattern is therefore
// a `regex` anchored with `^#lib/`, and exact names go in `paths` (plan-a decision 18).
//
// In a flat config the last matching object wins for a rule, so every file group below gets the
// full, combined list of restrictions that applies to it.
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig, globalIgnores } from 'eslint/config';
import svelte from 'eslint-plugin-svelte';
import globals from 'globals';
import ts from 'typescript-eslint';

/**
 * Deepest folder level under `components/ui/` that the per-depth relative rule covers (`ui/x.ts`
 * is 0, `ui/button/button.svelte` is 1). That rule is only needed for paths that step straight
 * out of `ui/` into a sibling (`../chat/X` from `ui/x.ts`), which never name `components/`; paths
 * that climb higher and come back through `components/<x>/` are caught at any depth. A primitive
 * deeper than this falls outside every `ui/` file group, so `tests/eslint/boundaries.test.ts`
 * fails if one appears.
 */
export const UI_MAX_DEPTH = 3;

const KIT_MESSAGE =
  'Kit 3 has no `$lib`, and `$env/*` and `$app/environment` are deprecated: use `#lib/…` and `$app/env/*` (RF-A14).';

/** @param {boolean} relative @returns {{ paths: object[], patterns: object[] }} */
function baseRestrictions(relative) {
  return {
    paths: [
      { name: '$lib', message: KIT_MESSAGE },
      { name: '$app/environment', message: KIT_MESSAGE },
      {
        name: '#lib/paraglide/runtime.js',
        importNames: ['setLocale'],
        message:
          'Never call Paraglide `setLocale`: switch language with a link to `localizedHref()` and `writeLocaleCookie()` (plan-a decision 2).',
      },
    ],
    patterns: [
      { group: ['$lib/**', '$env/**'], message: KIT_MESSAGE },
      ...(relative
        ? [
            {
              group: ['**/paraglide/runtime.js', '**/paraglide/runtime'],
              importNames: ['setLocale'],
              message: 'Never call Paraglide `setLocale` (plan-a decision 2).',
            },
          ]
        : []),
    ],
  };
}

const RENDERER_MESSAGE =
  'Only `FightStage.svelte` imports values from the renderer; elsewhere use `import type` (ADR-0009 D4).';
/** @param {boolean} relative */
const rendererRestriction = (relative) => [
  { regex: '^@torneo/renderer(?:/.*)?$', allowTypeImports: true, message: RENDERER_MESSAGE },
  ...(relative
    ? [{ group: ['**/packages/renderer/**'], allowTypeImports: true, message: RENDERER_MESSAGE }]
    : []),
];

const FEATURES_VALUE_MESSAGE =
  'Domain components receive data by props: only `import type` from features (ADR-0009 D4).';
/** @param {boolean} relative */
const featuresValueRestriction = (relative) => [
  { regex: '^#lib/features/', allowTypeImports: true, message: FEATURES_VALUE_MESSAGE },
  ...(relative
    ? [{ group: ['**/features/**'], allowTypeImports: true, message: FEATURES_VALUE_MESSAGE }]
    : []),
];

const UI_MESSAGE =
  'Primitives in `ui/` import nothing from features, domain components or Paraglide messages, not even types (ADR-0009 D4).';
/**
 * Everything outside `components/ui/` is off limits, not a closed list of domains, so a new
 * domain folder is covered on day one (TA-10). Relative paths leave `ui/` in two ways:
 * - climbing higher and coming back through `components/<x>/` (`../../components/chat/X`): the
 *   path names `components/`, so one regex covers every depth;
 * - climbing exactly one level more than the file's depth under `ui/` (`../chat/X` from `ui/x.ts`,
 *   `../../chat/X` from `ui/card/x.ts`): the path never names `components/`, so each depth gets
 *   its own regex.
 * Dot segments after a normal segment (`../button/../../chat`) and `./../` are rejected outright.
 * @param {boolean} relative @param {boolean} allowMessages @param {number} depth
 */
const uiRestriction = (relative, allowMessages, depth) => [
  { regex: '^#lib/features/', message: UI_MESSAGE },
  { regex: '^#lib/components/(?!ui/)', message: UI_MESSAGE },
  { regex: '^#lib/(?:.*/)?\\.\\.?(?:/|$)', message: UI_MESSAGE },
  ...(allowMessages ? [] : [{ regex: '^#lib/paraglide/', message: UI_MESSAGE }]),
  ...(relative
    ? [
        { group: ['**/features/**'], message: UI_MESSAGE },
        // Climbing to `lib/` (or higher) and coming back down through `components/<x>/`.
        { regex: '^\\.{1,2}/(?:.*/)?components/(?!ui(?:/|$))', message: UI_MESSAGE },
        // Stepping straight out of `ui/` into a sibling folder, which never names `components/`.
        { regex: `^(?:\\.\\./){${depth + 1}}(?!ui(?:/|$)|\\.\\./)`, message: UI_MESSAGE },
        {
          regex: '^\\./\\.\\.?/|^(?:\\.\\.?/)*(?:[^./][^/]*/)+\\.\\.?(?:/|$)',
          message: UI_MESSAGE,
        },
        ...(allowMessages ? [] : [{ group: ['**/paraglide/**'], message: UI_MESSAGE }]),
      ]
    : []),
];

/** @typedef {(relative: boolean) => object[]} Restriction */

const DYNAMIC_IMPORTS = 'torneo/no-restricted-dynamic-imports';

/**
 * Both import rules with the same restrictions: `no-restricted-imports` for static imports and its
 * `import()` twin below.
 * @param {boolean} relative
 * @param {Restriction[]} extra
 * @returns {Record<string, import('eslint').Linter.RuleEntry>}
 */
function restrictedImports(relative, extra) {
  const base = baseRestrictions(relative);
  /** @type {import('eslint').Linter.RuleEntry} */
  const entry = [
    'error',
    { paths: base.paths, patterns: [...base.patterns, ...extra.flatMap((f) => f(relative))] },
  ];
  return { '@typescript-eslint/no-restricted-imports': entry, [DYNAMIC_IMPORTS]: entry };
}

// `ts.plugin` is typed as a loose `CompatiblePlugin`, without `rules`.
/** @type {{ meta: import('eslint').Rule.RuleMetaData, create: import('eslint').Rule.RuleModule['create'] }} */
const restrictedImportsRule = /** @type {any} */ (ts.plugin).rules['no-restricted-imports'];

/**
 * `no-restricted-imports` does not see `import()` (TA-11). This rule takes the same options and
 * hands every `import()` to it as `import * as _ from '…'`, the way typescript-eslint already does
 * for `import x = require('…')`: paths, patterns and `importNames` apply as they are, and a
 * dynamic import is never type-only. A source that is not a string literal cannot be checked, so
 * it is not allowed.
 * @type {import('eslint').Rule.RuleModule}
 */
const dynamicImportsRule = {
  meta: {
    ...restrictedImportsRule.meta,
    docs: { description: 'Apply `no-restricted-imports` to `import()` (TA-11).' },
    messages: {
      ...restrictedImportsRule.meta.messages,
      nonLiteral:
        'The source of `import()` must be a string literal so the import boundaries can check it (TA-11).',
    },
  },
  create(context) {
    const listeners = /** @type {Record<string, (node: object) => void>} */ (
      /** @type {unknown} */ (restrictedImportsRule.create(/** @type {never} */ (context)))
    );
    return {
      ImportExpression(node) {
        const { source } = node;
        /** @type {string | undefined} */
        let value;
        if (source.type === 'Literal' && typeof source.value === 'string') value = source.value;
        else if (source.type === 'TemplateLiteral' && source.expressions.length === 0)
          value = source.quasis[0]?.value.cooked ?? undefined;
        if (value === undefined) {
          context.report({ node: source, messageId: 'nonLiteral' });
          return;
        }
        const specifier = { type: 'ImportNamespaceSpecifier', loc: node.loc, range: node.range };
        listeners.ImportDeclaration?.({
          ...node,
          type: 'ImportDeclaration',
          importKind: 'value',
          attributes: [],
          source: { ...source, type: 'Literal', value, raw: JSON.stringify(value) },
          specifiers: [specifier],
        });
      },
    };
  },
};

const URL_PROPS = '/^(origin|host|hostname|href|port|protocol)$/';
const FORWARDING_HEADERS = '/^(?:host|forwarded|x-forwarded-.*)$/i';
const ABSOLUTE_URL_MESSAGE =
  'Never build absolute URLs from the request (`Host` is client-controlled); use PUBLIC_ORIGIN (plan-a decision 20).';
const absoluteUrlSelectors = [
  `MemberExpression[object.type='MemberExpression'][object.property.name='url'][property.name=${URL_PROPS}]`,
  `MemberExpression[object.type='Identifier'][object.name='url'][property.name=${URL_PROPS}]`,
  "MemberExpression[object.type='Identifier'][object.name='request'][property.name='url']",
  "MemberExpression[object.type='MemberExpression'][object.property.name='request'][property.name='url']",
  "NewExpression[callee.name='URL'][arguments.length>=2]",
  "CallExpression[callee.property.name=/^(toString|toJSON)$/][callee.object.property.name='url']",
  "CallExpression[callee.property.name=/^(toString|toJSON)$/][callee.object.name='url']",
  "CallExpression[callee.name='String'][arguments.0.property.name='url']",
  "CallExpression[callee.name='String'][arguments.0.name='url']",
  "TemplateLiteral > MemberExpression.expressions[property.name='url']",
  "TemplateLiteral > Identifier.expressions[name='url']",
  // Destructuring `origin`, `host` or `href` out of a URL (TA-10).
  `VariableDeclarator[init.type='MemberExpression'][init.property.name='url'] > ObjectPattern > Property[key.name=${URL_PROPS}]`,
  `VariableDeclarator[init.type='Identifier'][init.name='url'] > ObjectPattern > Property[key.name=${URL_PROPS}]`,
  `AssignmentExpression[right.type='MemberExpression'][right.property.name='url'] > ObjectPattern > Property[key.name=${URL_PROPS}]`,
  `AssignmentExpression[right.type='Identifier'][right.name='url'] > ObjectPattern > Property[key.name=${URL_PROPS}]`,
  `Property[key.name='url'] > ObjectPattern > Property[key.name=${URL_PROPS}]`,
  // `new URL(event.url)` copies the client-controlled `Host` (TA-10).
  "NewExpression[callee.name='URL'] > MemberExpression.arguments[property.name='url']",
  "NewExpression[callee.name='URL'] > Identifier.arguments[name='url']",
  // PR #2 review: computed access, `URL.parse`, `'' + url` and `new Request(url)`.
  `MemberExpression[object.type='MemberExpression'][object.property.name='url'][property.value=${URL_PROPS}]`,
  `MemberExpression[object.type='Identifier'][object.name='url'][property.value=${URL_PROPS}]`,
  "CallExpression[callee.object.name='URL'][callee.property.name='parse'][arguments.length>=2]",
  "CallExpression[callee.object.name='URL'][callee.property.name='parse'] > MemberExpression.arguments[property.name='url']",
  "CallExpression[callee.object.name='URL'][callee.property.name='parse'] > Identifier.arguments[name='url']",
  "BinaryExpression[operator='+'] > MemberExpression[property.name='url']",
  "BinaryExpression[operator='+'] > Identifier[name='url']",
  "NewExpression[callee.name='Request'] > MemberExpression.arguments[property.name='url']",
  "NewExpression[callee.name='Request'] > Identifier.arguments[name='url']",
  // The `Host`, `Forwarded` and `X-Forwarded-*` headers are client-controlled too (TA-10). A
  // template literal as the name of a header to read is never needed, so it is not allowed.
  "CallExpression[callee.property.name='get'][callee.object.property.name='headers'][arguments.0.type='TemplateLiteral']",
  "CallExpression[callee.property.name='get'][callee.object.name='headers'][arguments.0.type='TemplateLiteral']",
  `MemberExpression[object.callee.property.name='fromEntries'][property.name=${FORWARDING_HEADERS}]`,
  `MemberExpression[object.callee.property.name='fromEntries'][property.value=${FORWARDING_HEADERS}]`,
  `CallExpression[callee.property.name='get'][arguments.0.value=${FORWARDING_HEADERS}]`,
  `MemberExpression[object.property.name='headers'][property.name=${FORWARDING_HEADERS}]`,
  `MemberExpression[object.property.name='headers'][property.value=${FORWARDING_HEADERS}]`,
  `MemberExpression[object.name='headers'][property.name=${FORWARDING_HEADERS}]`,
  `MemberExpression[object.name='headers'][property.value=${FORWARDING_HEADERS}]`,
].map((selector) => ({ selector, message: ABSOLUTE_URL_MESSAGE }));

/**
 * @param {{ relativePatterns?: boolean }} [options] `relativePatterns: false` drops the relative
 *   twins so tests can prove each `#lib` regex catches its case on its own.
 */
export function createConfig({ relativePatterns = true } = {}) {
  const r = relativePatterns;
  const SRC = 'src/**/*.{js,ts,svelte}';
  const COMPONENTS = 'src/lib/components/**/*.{js,ts,svelte}';
  const UI = 'src/lib/components/ui/**/*.{js,ts,svelte}';
  const A11Y = 'src/lib/components/ui/_a11y-messages.ts';
  const FIGHT_STAGE = 'src/lib/components/fight/FightStage.svelte';

  return defineConfig([
    globalIgnores([
      'build/',
      '.svelte-kit/',
      'src/lib/paraglide/',
      'node_modules/',
      'test-results/',
      'playwright-report/',
    ]),
    js.configs.recommended,
    ...ts.configs.recommended,
    ...svelte.configs.recommended,
    prettier,
    ...svelte.configs.prettier,
    {
      languageOptions: { globals: { ...globals.browser, ...globals.node } },
      plugins: { torneo: { rules: { 'no-restricted-dynamic-imports': dynamicImportsRule } } },
      rules: {
        'no-restricted-imports': 'off',
        // Our links are locale-prefixed paths built by `localizedHref()`, not route ids, and
        // primitives only forward `href`; `resolve()` from `$app/paths` does not apply here.
        'svelte/no-navigation-without-resolve': 'off',
        '@typescript-eslint/no-unused-vars': ['error', { argsIgnorePattern: '^_' }],
      },
    },
    {
      files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
      languageOptions: { parserOptions: { parser: ts.parser, extraFileExtensions: ['.svelte'] } },
    },
    // Imports: one combined rule per file group.
    {
      files: [SRC],
      ignores: [COMPONENTS],
      rules: restrictedImports(r, [rendererRestriction]),
    },
    {
      files: [COMPONENTS],
      ignores: [UI, FIGHT_STAGE],
      rules: restrictedImports(r, [rendererRestriction, featuresValueRestriction]),
    },
    {
      files: [FIGHT_STAGE],
      rules: restrictedImports(r, [featuresValueRestriction]),
    },
    ...Array.from({ length: UI_MAX_DEPTH + 1 }, (_, depth) => ({
      files: [`src/lib/components/ui/${'*/'.repeat(depth)}*.{js,ts,svelte}`],
      ignores: [A11Y],
      rules: restrictedImports(r, [
        rendererRestriction,
        /** @param {boolean} rel */ (rel) => uiRestriction(rel, false, depth),
      ]),
    })),
    {
      files: [A11Y],
      rules: restrictedImports(r, [
        rendererRestriction,
        /** @param {boolean} rel */ (rel) => uiRestriction(rel, true, 0),
      ]),
    },
    // Components never fetch (ADR-0009 D4).
    {
      files: [COMPONENTS],
      rules: {
        'no-restricted-globals': [
          'error',
          {
            name: 'fetch',
            message: 'Components do not fetch; data comes from features via props.',
          },
        ],
      },
    },
    // Logs only through src/lib/server/log.ts (RF-A19).
    {
      files: [SRC],
      ignores: ['src/lib/server/log.ts'],
      rules: { 'no-console': 'error' },
    },
    // Absolute URLs only from PUBLIC_ORIGIN (plan-a decision 20). Tests read fixtures with
    // `new URL(path, import.meta.url)`, so they are exempt.
    {
      files: ['src/**/*.{js,ts,svelte}'],
      ignores: ['src/**/*.test.ts'],
      rules: { 'no-restricted-syntax': ['error', ...absoluteUrlSelectors] },
    },
  ]);
}

export default createConfig();
