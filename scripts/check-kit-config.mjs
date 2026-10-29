#!/usr/bin/env node
// RF-A10 and plan-a decision 17: Kit 3 config must never weaken or pin CSRF origin checks.
// Fails if apps/web/vite.config.ts mentions `trustedOrigins`, `checkOrigin` or `paths.origin`
// (or an `origin:` key inside `paths`), or if apps/web/svelte.config.js exists.
// ADR-0012 B1/B3/B4 (consequence 6): the `csp` option is an exact allowlist, not a blocklist.
import { existsSync, readFileSync } from 'node:fs';
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

/**
 * Splits the literal that follows `csp:` into tokens. Only object and array punctuation, plain
 * string literals and identifiers (as keys) exist; anything else (spreads, template literals,
 * calls, escapes) is reported, so the check always sees the whole configuration.
 * @param {string} source
 * @param {number} start index of the `{` after `csp:`
 * @param {number} firstLine 1-based line of `start`
 * @returns {{ tokens: {kind: string, value: string, line: number}[], error: string | null }}
 */
function tokenize(source, start, firstLine) {
  /** @type {{kind: string, value: string, line: number}[]} */
  const tokens = [];
  let line = firstLine;
  let depth = 0;
  let i = start;
  while (i < source.length) {
    const ch = source[i];
    if (ch === '\n') {
      line++;
      i++;
    } else if (/\s/.test(ch)) {
      i++;
    } else if (source.startsWith('//', i)) {
      while (i < source.length && source[i] !== '\n') i++;
    } else if (source.startsWith('/*', i)) {
      const end = source.indexOf('*/', i + 2);
      if (end === -1) return { tokens, error: `line ${line}: unterminated comment` };
      for (const c of source.slice(i, end)) if (c === '\n') line++;
      i = end + 2;
    } else if ('{}[]:,'.includes(ch)) {
      tokens.push({ kind: ch, value: ch, line });
      if (ch === '{' || ch === '[') depth++;
      if (ch === '}' || ch === ']') depth--;
      i++;
      if (depth === 0) return { tokens, error: null };
    } else if (ch === "'" || ch === '"') {
      const end = source.indexOf(ch, i + 1);
      const value = end === -1 ? '' : source.slice(i + 1, end);
      if (end === -1 || /[\\\n]/.test(value)) {
        return { tokens, error: `line ${line}: only plain string literals are allowed` };
      }
      tokens.push({ kind: 'string', value, line });
      i = end + 1;
    } else if (/[A-Za-z_$]/.test(ch)) {
      const m = /^[A-Za-z_$][\w$]*/.exec(source.slice(i));
      tokens.push({ kind: 'ident', value: m[0], line });
      i += m[0].length;
    } else {
      return {
        tokens,
        error: `line ${line}: \`${ch}\` is not allowed (the value must be a literal)`,
      };
    }
  }
  return { tokens, error: 'unterminated literal' };
}

/**
 * Parses an object of string arrays or strings, one level of nesting, from tokens.
 * @returns {{ value: any, error: string | null }}
 */
function parseLiteral(tokens) {
  let pos = 0;
  const peek = () => tokens[pos];
  const fail = (msg) => {
    throw new Error(`line ${peek()?.line ?? tokens.at(-1)?.line}: ${msg}`);
  };
  const expect = (kind) => {
    if (peek()?.kind !== kind) fail(`expected \`${kind}\``);
    return tokens[pos++];
  };
  /** @returns {any} */
  const value = () => {
    const t = peek();
    if (t?.kind === 'string') {
      pos++;
      return { value: t.value, line: t.line };
    }
    if (t?.kind === '[') {
      pos++;
      const items = [];
      while (peek()?.kind !== ']') {
        const item = expect('string');
        items.push({ value: item.value, line: item.line });
        if (peek()?.kind === ',') pos++;
        else if (peek()?.kind !== ']') fail('expected `,` or `]`');
      }
      pos++;
      return { array: items, line: t.line };
    }
    if (t?.kind === '{') {
      pos++;
      /** @type {Map<string, any>} */
      const entries = new Map();
      while (peek()?.kind !== '}') {
        const key = peek();
        if (key?.kind !== 'string' && key?.kind !== 'ident') fail('expected a key');
        pos++;
        expect(':');
        if (entries.has(key.value)) fail(`duplicate key \`${key.value}\``);
        entries.set(key.value, { ...value(), keyLine: key.line });
        if (peek()?.kind === ',') pos++;
        else if (peek()?.kind !== '}') fail('expected `,` or `}`');
      }
      pos++;
      return { object: entries, line: t.line };
    }
    return fail(`\`${t?.value ?? 'end'}\` is not allowed (the value must be a literal)`);
  };
  try {
    return { value: value(), error: null };
  } catch (error) {
    return { value: null, error: /** @type {Error} */ (error).message };
  }
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
  // Blank line comments as checkViteConfigSource strips them, keeping every offset, so a `csp` in
  // a comment does not count.
  const stripped = source
    .split('\n')
    .map((l) =>
      l.replace(/(^|[^:])(\/\/.*)$/, (_, pre, comment) => pre + ' '.repeat(comment.length)),
    )
    .join('\n');
  const mentions = [...stripped.matchAll(/\bcsp\b/g)];
  const lineOf = (index) => source.slice(0, index).split('\n').length;
  if (mentions.length === 0) return [`${file}:1: ${rule}: the \`csp\` option is missing`];
  if (mentions.length > 1) {
    return mentions
      .slice(1)
      .map((m) => `${file}:${lineOf(m.index)}: ${rule}: \`csp\` must appear exactly once`);
  }
  const key = /\bcsp\s*:\s*\{/y;
  key.lastIndex = mentions[0].index;
  if (!key.test(stripped)) {
    return [`${file}:${lineOf(mentions[0].index)}: ${rule}: must be written \`csp: { … }\``];
  }
  const open = key.lastIndex - 1;
  const { tokens, error: tokenError } = tokenize(source, open, lineOf(open));
  const parsed = tokenError ? { value: null, error: tokenError } : parseLiteral(tokens);
  if (parsed.error) return [`${file}: ${rule}: ${parsed.error}`];

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
    const base = CSP_BASE[name] ?? CSP_EXTENSIONS.extraDirectives[name];
    if (!base) {
      problems.push(`${at(entry.keyLine)}: directive \`${name}\` is not allowed`);
      continue;
    }
    if (!entry.array) {
      problems.push(`${at(entry.keyLine)}: \`${name}\` must be an array of sources`);
      continue;
    }
    const allowed = new Set([...base, ...(CSP_EXTENSIONS.extraSources[name] ?? [])]);
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
