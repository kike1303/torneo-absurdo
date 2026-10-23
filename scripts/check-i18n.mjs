#!/usr/bin/env node
// RF-A15 and plan-a decision 7. Fails if:
// - a message key exists in one locale and not in another, or a message is empty;
// - a `.svelte` file outside src/routes/dev/** has literal visible text (text nodes with letters,
//   or literal `aria-label`, `title`, `alt` or `placeholder`), found with svelte/compiler. That
//   includes string literals that an expression outputs as is: `{"Save"}`, `{`Hi ${n}`}`,
//   `{ok ? "Yes" : "No"}`, `aria-label={"Close"}` (TA-10);
// - an entry of `modules` in project.inlang/settings.json starts with `http` (no remote plugins).
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse } from 'svelte/compiler';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const LETTER = /\p{L}/u;
const TEXT_ATTRIBUTES = new Set(['aria-label', 'title', 'alt', 'placeholder']);

/** @param {string} webDir */
function rel(webDir, file) {
  return path.relative(ROOT, path.join(webDir, file)).split(path.sep).join('/');
}

/** @param {string} source @param {number} index */
function lineAt(source, index) {
  let line = 1;
  for (let i = 0; i < index && i < source.length; i++) if (source[i] === '\n') line++;
  return line;
}

/** Message parity and empty messages. */
function checkMessages(webDir, problems) {
  const dir = path.join(webDir, 'messages');
  if (!existsSync(dir)) {
    problems.push(`${rel(webDir, 'messages')}:1: messages directory is missing`);
    return;
  }
  /** @type {Map<string, { file: string, source: string, messages: Record<string, unknown> }>} */
  const locales = new Map();
  for (const entry of readdirSync(dir)
    .filter((f) => f.endsWith('.json'))
    .sort()) {
    const file = rel(webDir, path.join('messages', entry));
    const source = readFileSync(path.join(dir, entry), 'utf8');
    try {
      locales.set(path.basename(entry, '.json'), { file, source, messages: JSON.parse(source) });
    } catch (error) {
      problems.push(
        `${file}:1: invalid JSON (${error instanceof Error ? error.message : 'parse error'})`,
      );
    }
  }
  const keysOf = (messages) => Object.keys(messages).filter((key) => !key.startsWith('$'));
  const all = new Set([...locales.values()].flatMap(({ messages }) => keysOf(messages)));
  for (const [locale, { file, source, messages }] of locales) {
    for (const key of all) {
      if (!(key in messages))
        problems.push(`${file}:1: missing key \`${key}\` in locale \`${locale}\``);
    }
    for (const key of keysOf(messages)) {
      const value = messages[key];
      if (typeof value !== 'string' || value.trim() === '') {
        const index = source.indexOf(`"${key}"`);
        problems.push(`${file}:${lineAt(source, Math.max(index, 0))}: empty message \`${key}\``);
      }
    }
  }
}

/** No remote modules in the inlang project (plan-a decision 7). */
function checkSettings(webDir, problems) {
  const file = path.join(webDir, 'project.inlang', 'settings.json');
  if (!existsSync(file)) return;
  const source = readFileSync(file, 'utf8');
  /** @type {{ modules?: unknown }} */
  let settings;
  try {
    settings = JSON.parse(source);
  } catch {
    problems.push(`${rel(webDir, 'project.inlang/settings.json')}:1: invalid JSON`);
    return;
  }
  const modules = Array.isArray(settings.modules) ? settings.modules : [];
  for (const entry of modules) {
    if (typeof entry === 'string' && /^\s*http/i.test(entry)) {
      const index = source.indexOf(entry);
      problems.push(
        `${rel(webDir, 'project.inlang/settings.json')}:${lineAt(source, Math.max(index, 0))}: remote module \`${entry}\`; install the plugin and use a local path`,
      );
    }
  }
}

/**
 * String literals with letters that an expression renders as is: the expression itself, the
 * text of a template, the elements of an array, both branches of `?:`, both sides of `&&`/`||`/`??` and of a `+`. Literals
 * passed to calls or compared (`m.x()`, `fmt(n, "en")`, `a === "b"`) are code, not text.
 * @returns {{ text: string, start: number }[]}
 */
function renderedLiterals(expr) {
  if (!expr || typeof expr !== 'object') return [];
  switch (expr.type) {
    case 'Literal':
      return typeof expr.value === 'string' && LETTER.test(expr.value)
        ? [{ text: expr.value, start: expr.start }]
        : [];
    case 'TemplateLiteral': {
      const quasis = expr.quasis.map((q) => q.value.cooked ?? q.value.raw);
      const own = LETTER.test(quasis.join(''))
        ? [{ text: quasis.join('…'), start: expr.start }]
        : [];
      return [...own, ...expr.expressions.flatMap(renderedLiterals)];
    }
    case 'ArrayExpression': // `{["a", "b"]}` renders as `a,b`
      return expr.elements.flatMap(renderedLiterals);
    case 'ConditionalExpression':
      return [...renderedLiterals(expr.consequent), ...renderedLiterals(expr.alternate)];
    case 'LogicalExpression':
      return [...renderedLiterals(expr.left), ...renderedLiterals(expr.right)];
    case 'BinaryExpression':
      return expr.operator === '+'
        ? [...renderedLiterals(expr.left), ...renderedLiterals(expr.right)]
        : [];
    default:
      return [];
  }
}

/** Walks the modern Svelte AST looking for literal visible text. */
function checkSvelte(file, source, problems) {
  let ast;
  try {
    ast = parse(source, { modern: true, filename: file });
  } catch (error) {
    problems.push(`${file}:1: cannot parse (${error instanceof Error ? error.message : 'error'})`);
    return;
  }
  const visit = (node) => {
    if (!node || typeof node !== 'object') return;
    if (Array.isArray(node)) {
      for (const child of node) visit(child);
      return;
    }
    if (node.type === 'Text') {
      if (LETTER.test(node.data))
        problems.push(
          `${file}:${lineAt(source, node.start)}: literal text \`${node.data.trim().slice(0, 40)}\`; use a Paraglide message`,
        );
      return;
    }
    if (node.type === 'Comment') return;
    if (node.type === 'ExpressionTag' || node.type === 'HtmlTag') {
      for (const { text, start } of renderedLiterals(node.expression))
        problems.push(
          `${file}:${lineAt(source, start)}: literal text \`${text.trim().slice(0, 40)}\`; use a Paraglide message`,
        );
      return;
    }
    if (node.type === 'Attribute') {
      if (TEXT_ATTRIBUTES.has(node.name.toLowerCase())) {
        // `attr="…"` is an array of Text and ExpressionTag parts; `attr={…}` is one ExpressionTag.
        const parts = Array.isArray(node.value) ? node.value : [node.value];
        for (const part of parts) {
          if (!part || typeof part !== 'object') continue;
          const literals =
            part.type === 'Text'
              ? LETTER.test(part.data)
                ? [{ text: part.data, start: part.start }]
                : []
              : part.type === 'ExpressionTag'
                ? renderedLiterals(part.expression)
                : [];
          for (const { text, start } of literals)
            problems.push(
              `${file}:${lineAt(source, start)}: literal \`${node.name}\` \`${text.slice(0, 40)}\`; use a Paraglide message`,
            );
        }
      }
      return; // other attribute values (class, href, …) are not visible text
    }
    for (const [key, value] of Object.entries(node)) {
      if (key === 'instance' || key === 'module' || key === 'css' || key === 'options') continue;
      if (value && typeof value === 'object') visit(value);
    }
  };
  visit(ast.fragment);
}

function collectSvelte(dir, base, out) {
  if (!existsSync(dir)) return;
  for (const entry of readdirSync(dir)) {
    const full = path.join(dir, entry);
    if (statSync(full).isDirectory()) {
      if (entry !== 'node_modules' && entry !== 'paraglide') collectSvelte(full, base, out);
    } else if (entry.endsWith('.svelte')) out.push(full);
  }
}

/**
 * @param {{ webDir: string }} options absolute path to the app (apps/web or a fixture)
 * @returns {string[]}
 */
export function checkI18n({ webDir }) {
  /** @type {string[]} */
  const problems = [];
  checkMessages(webDir, problems);
  checkSettings(webDir, problems);
  /** @type {string[]} */
  const files = [];
  collectSvelte(path.join(webDir, 'src'), webDir, files);
  const devDir = path.join(webDir, 'src', 'routes', 'dev') + path.sep;
  for (const full of files.sort()) {
    if (full.startsWith(devDir)) continue; // catalog demo texts are exempt (ADR-0009 D5)
    checkSvelte(
      path.relative(ROOT, full).split(path.sep).join('/'),
      readFileSync(full, 'utf8'),
      problems,
    );
  }
  return problems;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const webDir = process.argv[2] ? path.resolve(process.argv[2]) : path.join(ROOT, 'apps/web');
  const problems = checkI18n({ webDir });
  if (problems.length > 0) {
    console.error(`check-i18n: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('check-i18n: ok');
}
