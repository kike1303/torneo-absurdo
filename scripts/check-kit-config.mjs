#!/usr/bin/env node
// RF-A10 and plan-a decision 17: Kit 3 config must never weaken or pin CSRF origin checks.
// Fails if apps/web/vite.config.ts mentions `trustedOrigins`, `checkOrigin` or `paths.origin`
// (or an `origin:` key inside `paths`), or if apps/web/svelte.config.js exists.
// ADR-0012 B1/B3/B4 (consequence 6): the `csp` option is an exact allowlist, not a blocklist.
import { existsSync, readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

/** ADR-0012 B1, as Kit spells the sources (unquoted keywords). */
export const CSP_BASE = Object.freeze({
  'default-src': ['self'],
  'script-src': ['self'],
  'style-src': ['self', 'unsafe-inline'],
  'img-src': ['self', 'data:', 'blob:'],
  'font-src': ['self'],
  'connect-src': ['self'],
  'object-src': ['none'],
  'base-uri': ['self'],
  'form-action': ['self'],
  'frame-ancestors': ['none'],
});

/**
 * ADR-0012 B4, the only differences from B1: an extra source in an existing directive, or a new
 * directive with exactly these sources.
 */
export const CSP_EXTENSIONS = Object.freeze({
  extraSources: { 'script-src': ['wasm-unsafe-eval'] },
  extraDirectives: { 'worker-src': ['self'] },
});

/** Kit's plugin factory: the only import allowed from this module (security-reviewer on #50). */
const KIT_VITE = '@sveltejs/kit/vite';

// The TypeScript compiler apps/web already pins; the root has no copy of its own.
const ts = createRequire(path.join(ROOT, 'apps/web/package.json'))('typescript');

/**
 * Converts a literal of the csp option to `{ value }`, `{ array }` or `{ object }`, with lines.
 * Only plain keys, string literals, arrays of them and nested objects are accepted, so the check
 * always reads the value Kit receives.
 * @param {any} node
 * @param {any} sf
 * @returns {any}
 */
function literal(node, sf) {
  const line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
  if (ts.isStringLiteral(node)) return { value: node.text, line };
  if (ts.isArrayLiteralExpression(node)) {
    return {
      array: node.elements.map((e) => {
        if (!ts.isStringLiteral(e))
          throw new LiteralError(e, sf, 'sources must be string literals');
        return { value: e.text, line: sf.getLineAndCharacterOfPosition(e.getStart(sf)).line + 1 };
      }),
      line,
    };
  }
  if (ts.isObjectLiteralExpression(node)) {
    /** @type {Map<string, any>} */
    const object = new Map();
    for (const property of node.properties) {
      const name = plainKey(property);
      if (name === null)
        throw new LiteralError(property, sf, 'keys must be plain (no spread or computed key)');
      if (object.has(name)) throw new LiteralError(property, sf, `repeats \`${name}\``);
      const keyLine = sf.getLineAndCharacterOfPosition(property.getStart(sf)).line + 1;
      object.set(name, { ...literal(property.initializer, sf), keyLine });
    }
    return { object, line };
  }
  throw new LiteralError(
    node,
    sf,
    `\`${node.getText(sf)}\` is not allowed (the value must be a literal)`,
  );
}

class LiteralError extends Error {
  constructor(node, sf, message) {
    super(message);
    this.line = sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
  }
}

/** Name of a `key: value` property with an identifier or string key; null for anything else. */
function plainKey(property) {
  if (!ts.isPropertyAssignment(property)) return null;
  const { name } = property;
  return ts.isIdentifier(name) || ts.isStringLiteral(name) ? name.text : null;
}

/**
 * Finds the csp literal Kit receives: `sveltekit` imported once, by that name, from
 * `@sveltejs/kit/vite`, and called once as an element of `plugins`, with one object literal whose
 * keys are all plain and unique. Any other use of the name or of the module fails.
 * @returns {{ node: any } | { error: string, line: number }}
 */
function locateCsp(sf) {
  const lineOf = (node) => sf.getLineAndCharacterOfPosition(node.getStart(sf)).line + 1;
  /** @type {any[]} */
  const names = [];
  /** @type {any[]} */
  const modules = [];
  /** @type {any[]} */
  const dynamicImports = [];
  const visit = (node) => {
    if (ts.isIdentifier(node) && node.text === 'sveltekit') names.push(node);
    if (ts.isStringLiteralLike(node) && node.text === KIT_VITE) modules.push(node);
    if (ts.isCallExpression(node) && node.expression.kind === ts.SyntaxKind.ImportKeyword) {
      dynamicImports.push(node);
    }
    ts.forEachChild(node, visit);
  };
  visit(sf);
  const specifier = names.find((n) => ts.isImportSpecifier(n.parent) && !n.parent.propertyName);
  const declaration = specifier?.parent.parent.parent.parent;
  if (
    !specifier ||
    modules.length !== 1 ||
    !ts.isImportDeclaration(declaration) ||
    declaration.moduleSpecifier !== modules[0] ||
    declaration.importClause.name ||
    declaration.importClause.isTypeOnly ||
    declaration.importClause.namedBindings.elements.length !== 1
  ) {
    return {
      error: `\`sveltekit\` must be imported once, unaliased and alone, from '${KIT_VITE}'`,
      line: modules[1] ? lineOf(modules[1]) : 1,
    };
  }
  const uses = names.filter((n) => n !== specifier);
  const call = uses[0]?.parent;
  if (uses.length !== 1 || !ts.isCallExpression(call) || call.expression !== uses[0]) {
    return {
      error: '`sveltekit` must be called exactly once and used nowhere else',
      line: lineOf(uses[1] ?? uses[0] ?? specifier),
    };
  }
  // The `plugins` of the exported config, `export default { … }` or `defineConfig({ … })`, so a
  // decoy config elsewhere in the file does not count (security-reviewer round 3 on #50).
  const array = call.parent;
  const config = array.parent?.parent;
  const exported =
    config?.parent && ts.isCallExpression(config.parent)
      ? ts.isIdentifier(config.parent.expression) &&
        config.parent.expression.text === 'defineConfig' &&
        config.parent.arguments.length === 1 &&
        ts.isExportAssignment(config.parent.parent)
      : ts.isExportAssignment(config?.parent);
  if (
    !ts.isArrayLiteralExpression(array) ||
    !ts.isPropertyAssignment(array.parent) ||
    plainKey(array.parent) !== 'plugins' ||
    !ts.isObjectLiteralExpression(config) ||
    !exported
  ) {
    return {
      error: '`sveltekit(…)` must be an element of the exported config `plugins: [ … ]`',
      line: lineOf(call),
    };
  }
  if (dynamicImports.length > 0) {
    return {
      error: '`import()` is not allowed in vite.config.ts',
      line: lineOf(dynamicImports[0]),
    };
  }
  const [options, ...rest] = call.arguments;
  if (!options || rest.length > 0 || !ts.isObjectLiteralExpression(options)) {
    return { error: '`sveltekit(` must receive one object literal', line: lineOf(call) };
  }
  /** @type {Set<string>} */
  const keys = new Set();
  let csp = null;
  for (const property of options.properties) {
    const name = ts.isShorthandPropertyAssignment(property)
      ? property.name.text
      : plainKey(property);
    if (name === null) {
      return {
        error: '`sveltekit({ … })` keys must be plain (no spread or computed key)',
        line: lineOf(property),
      };
    }
    if (keys.has(name))
      return { error: `\`sveltekit({ … })\` repeats \`${name}\``, line: lineOf(property) };
    keys.add(name);
    if (name === 'csp') csp = property;
  }
  if (!csp) return { error: 'the `csp` option is missing', line: lineOf(options) };
  if (!ts.isPropertyAssignment(csp) || !ts.isObjectLiteralExpression(csp.initializer)) {
    return { error: 'must be written `csp: { … }`', line: lineOf(csp) };
  }
  return { node: csp.initializer };
}

/**
 * ADR-0012 consequence 6: compares the `csp` option of vite.config.ts with B1 plus, at most, the
 * B4 extensions. Every other option, directive or source fails, whether or not B3 names it.
 * @param {string} source contents of vite.config.ts
 * @param {string} file label used in messages
 * @returns {string[]} problems, as `file:line: message`
 */
export function checkCspSource(source, file) {
  const rule = 'csp (ADR-0012 B1/B4)';
  const sf = ts.createSourceFile(file, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
  if (sf.parseDiagnostics.length > 0) {
    const d = sf.parseDiagnostics[0];
    const line = sf.getLineAndCharacterOfPosition(d.start).line + 1;
    return [`${file}:${line}: ${rule}: not valid TypeScript`];
  }
  const located = locateCsp(sf);
  if ('error' in located) return [`${file}:${located.line}: ${rule}: ${located.error}`];
  /** @type {{ value: any, error: string | null }} */
  let parsed;
  try {
    parsed = { value: literal(located.node, sf), error: null };
  } catch (error) {
    if (!(error instanceof LiteralError)) throw error;
    return [`${file}:${error.line}: ${rule}: ${error.message}`];
  }

  /** @type {string[]} */
  const problems = [];
  const at = (line) => `${file}:${line}: ${rule}`;
  const csp = parsed.value.object;
  for (const [name, entry] of csp) {
    if (name !== 'mode' && name !== 'directives') {
      problems.push(`${at(entry.keyLine)}: option \`${name}\` is not allowed`);
    }
  }
  const mode = csp.get('mode');
  if (mode?.value !== 'auto') {
    problems.push(`${at(mode?.line ?? parsed.value.line)}: \`mode\` must be 'auto'`);
  }
  const directives = csp.get('directives');
  if (!directives?.object) {
    problems.push(`${at(directives?.line ?? parsed.value.line)}: \`directives\` must be an object`);
    return problems;
  }
  for (const name of Object.keys(CSP_BASE)) {
    if (!directives.object.has(name)) {
      problems.push(`${at(directives.line)}: directive \`${name}\` is missing`);
    }
  }
  for (const [name, entry] of directives.object) {
    const base = Object.hasOwn(CSP_BASE, name)
      ? CSP_BASE[name]
      : Object.hasOwn(CSP_EXTENSIONS.extraDirectives, name)
        ? CSP_EXTENSIONS.extraDirectives[name]
        : undefined;
    if (!base) {
      problems.push(`${at(entry.keyLine)}: directive \`${name}\` is not allowed`);
      continue;
    }
    if (!entry.array) {
      problems.push(`${at(entry.keyLine)}: \`${name}\` must be an array of sources`);
      continue;
    }
    const extra = Object.hasOwn(CSP_EXTENSIONS.extraSources, name)
      ? CSP_EXTENSIONS.extraSources[name]
      : [];
    const allowed = new Set([...base, ...extra]);
    const seen = new Set();
    for (const source of entry.array) {
      if (seen.has(source.value)) {
        problems.push(`${at(source.line)}: \`${name}\` repeats '${source.value}'`);
      } else if (!allowed.has(source.value)) {
        problems.push(`${at(source.line)}: \`${name}\` must not contain '${source.value}'`);
      }
      seen.add(source.value);
    }
    for (const required of base) {
      if (!seen.has(required)) {
        problems.push(`${at(entry.keyLine)}: \`${name}\` must contain '${required}'`);
      }
    }
  }
  return problems;
}

/**
 * @param {string} source contents of vite.config.ts
 * @param {string} file label used in messages
 * @returns {string[]} problems, as `file:line: message`
 */
export function checkViteConfigSource(source, file) {
  /** @type {string[]} */
  const problems = [];
  const lines = source.split('\n');
  /** @type {number[]} stack of brace depths where a `paths: {` block was opened */
  const pathsDepths = [];
  let depth = 0;

  lines.forEach((raw, index) => {
    // Strip line comments, but not the `//` of a URL scheme such as `https://`.
    const line = raw.replace(/(^|[^:])\/\/.*$/, '$1');
    const at = `${file}:${index + 1}`;
    if (/\btrustedOrigins\b/.test(line)) {
      problems.push(`${at}: \`trustedOrigins\` is forbidden (it can disable Kit's CSRF check)`);
    }
    if (/\bcheckOrigin\b/.test(line)) {
      problems.push(`${at}: \`checkOrigin\` is forbidden (removed in Kit 3)`);
    }
    if (/\bpaths\s*\.\s*origin\b/.test(line)) {
      problems.push(`${at}: \`paths.origin\` is forbidden (bakes the host into the image)`);
    }
    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      if (ch === '{') {
        depth++;
        if (/\bpaths\s*:\s*$/.test(line.slice(0, i))) pathsDepths.push(depth);
      } else if (ch === '}') {
        if (pathsDepths.at(-1) === depth) pathsDepths.pop();
        depth--;
      }
    }
    if (pathsDepths.length > 0 && /(^|[\s{,])['"]?origin['"]?\s*:/.test(line)) {
      problems.push(
        `${at}: \`origin\` inside \`paths\` is forbidden (bakes the host into the image)`,
      );
    }
  });
  return problems;
}

/**
 * @param {string} webDir absolute path to apps/web
 * @returns {string[]}
 */
export function checkKitConfig(webDir) {
  /** @type {string[]} */
  const problems = [];
  const svelteConfig = path.join(webDir, 'svelte.config.js');
  if (existsSync(svelteConfig)) {
    problems.push(
      `${path.relative(ROOT, svelteConfig)}:1: svelte.config.js must not exist in Kit 3 (config lives in vite.config.ts)`,
    );
  }
  const viteConfig = path.join(webDir, 'vite.config.ts');
  if (!existsSync(viteConfig)) {
    problems.push(`${path.relative(ROOT, viteConfig)}:1: vite.config.ts is missing`);
    return problems;
  }
  const source = readFileSync(viteConfig, 'utf8');
  const label = path.relative(ROOT, viteConfig);
  problems.push(...checkViteConfigSource(source, label), ...checkCspSource(source, label));
  return problems;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const webDir = process.argv[2] ? path.resolve(process.argv[2]) : path.join(ROOT, 'apps/web');
  const problems = checkKitConfig(webDir);
  if (problems.length > 0) {
    console.error(`check-kit-config: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('check-kit-config: ok');
}
