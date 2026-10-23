#!/usr/bin/env node
// ADR-0009 D1 and D6, RF-A13: no loose colors outside the tokens. Scans .svelte, .ts, .css and
// .svg under apps/web/src, except src/lib/theme/tokens.css, src/lib/theme/theme-color.ts and the
// generated src/lib/paraglide/**. Prints `file:line: message` and exits 1 on any problem.
import { readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const EXTENSIONS = new Set(['.svelte', '.ts', '.css', '.svg']);
const EXEMPT = ['lib/theme/tokens.css', 'lib/theme/theme-color.ts'];
const SKIP_DIRS = new Set(['paraglide', 'node_modules']);

const PALETTE_NAMES =
  'slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose|black|white';
const UTILITY_PREFIXES =
  'bg|text|border|ring|outline|fill|stroke|from|to|via|decoration|shadow|accent|caret|divide|placeholder';
const RAW_PALETTE_CLASS = new RegExp(
  `(?<![\\w-])(?:[\\w-]+:)*(?:${UTILITY_PREFIXES})-(?:[xytrblse]-)?(?:${PALETTE_NAMES})(?:-\\d{2,3})?(?:\\/\\d+)?(?![\\w-])`,
  'g',
);
const ARBITRARY_COLOR = /-\[(?:#|rgba?\(|hsla?\(|oklch\(|oklab\(|lab\(|lch\(|hwb\(|color\()/g;
const RING_OPACITY = /(?<![\w-])(?:[\w-]+:)*ring-ring\/(?:\d+|\[[^\]]*\])/g;
const PALETTE_VAR = /--palette-[\w-]+/g;
const OUTLINE_NONE = /(?<![\w-])(?:[\w-]+:)*outline-none(?![\w-])/;
const RING_REPLACEMENT = /(?<![\w-])(?:focus-visible|focus|focus-within):ring(?:-|\b)/;

const COLOR_FUNCTION = /(?<![\w-])(?:rgba?|hsla?|oklch|oklab|lab|lch|hwb|color)\(/i;
const HEX = /#(?:[0-9a-f]{8}|[0-9a-f]{6}|[0-9a-f]{3,4})(?![\w-])/i;
const NAMED_COLORS = new Set(
  (
    'aliceblue antiquewhite aqua aquamarine azure beige bisque black blanchedalmond blue ' +
    'blueviolet brown burlywood cadetblue chartreuse chocolate coral cornflowerblue cornsilk ' +
    'crimson cyan darkblue darkcyan darkgoldenrod darkgray darkgreen darkgrey darkkhaki ' +
    'darkmagenta darkolivegreen darkorange darkorchid darkred darksalmon darkseagreen ' +
    'darkslateblue darkslategray darkslategrey darkturquoise darkviolet deeppink deepskyblue ' +
    'dimgray dimgrey dodgerblue firebrick floralwhite forestgreen fuchsia gainsboro ghostwhite ' +
    'gold goldenrod gray green greenyellow grey honeydew hotpink indianred indigo ivory khaki ' +
    'lavender lavenderblush lawngreen lemonchiffon lightblue lightcoral lightcyan ' +
    'lightgoldenrodyellow lightgray lightgreen lightgrey lightpink lightsalmon lightseagreen ' +
    'lightskyblue lightslategray lightslategrey lightsteelblue lightyellow lime limegreen linen ' +
    'magenta maroon mediumaquamarine mediumblue mediumorchid mediumpurple mediumseagreen ' +
    'mediumslateblue mediumspringgreen mediumturquoise mediumvioletred midnightblue mintcream ' +
    'mistyrose moccasin navajowhite navy oldlace olive olivedrab orange orangered orchid ' +
    'palegoldenrod palegreen paleturquoise palevioletred papayawhip peachpuff peru pink plum ' +
    'powderblue purple rebeccapurple red rosybrown royalblue saddlebrown salmon sandybrown ' +
    'seagreen seashell sienna silver skyblue slateblue slategray slategrey snow springgreen ' +
    'steelblue tan teal thistle tomato turquoise violet wheat white whitesmoke yellow yellowgreen'
  ).split(' '),
);
const COLOR_PROPERTY =
  /^(?:color|background(?:-color)?|border(?:-[a-z-]+)?|outline(?:-color)?|fill|stroke|box-shadow|text-shadow|caret-color|accent-color|text-decoration(?:-color)?|column-rule(?:-color)?)$/i;
const SVG_COLOR_ATTRIBUTE =
  /\b(fill|stroke|stop-color|flood-color|lighting-color|color)\s*=\s*(["'])(.*?)\2/gi;
const SVG_ALLOWED = /^(?:currentColor|none|url\(#[^)]*\)|var\(--[\w-]+\))$/;

/** 1-based line of a character offset. */
function lineAt(source, index) {
  let line = 1;
  for (let i = 0; i < index && i < source.length; i++) if (source[i] === '\n') line++;
  return line;
}

/** Problems in one CSS text (a `.css` file, a `<style>` block or a `style="…"` value). */
function checkCssText(css, offset, source, report) {
  const declaration = /([a-z-]+)\s*:\s*([^;{}]+)/gi;
  for (const match of css.matchAll(declaration)) {
    const [, property, value] = match;
    if (property.startsWith('--')) continue;
    const at = offset + (match.index ?? 0);
    const cleaned = value.replace(/url\([^)]*\)/gi, '');
    if (HEX.test(cleaned))
      report(lineAt(source, at), `color literal \`${cleaned.match(HEX)[0]}\` in CSS; use a token`);
    else if (COLOR_FUNCTION.test(cleaned))
      report(
        lineAt(source, at),
        `color function in CSS (\`${property}: ${value.trim()}\`); use a token`,
      );
    else if (COLOR_PROPERTY.test(property)) {
      for (const word of cleaned.toLowerCase().split(/[^a-z]+/)) {
        if (NAMED_COLORS.has(word)) {
          report(lineAt(source, at), `named color \`${word}\` in \`${property}\`; use a token`);
          break;
        }
      }
    }
  }
}

/** Every quoted string in a file, with its offset. */
function* strings(source) {
  const re = /(["'`])((?:\\.|(?!\1)[^\\])*?)\1/gs;
  for (const match of source.matchAll(re)) yield { text: match[2], index: match.index ?? 0 };
}

/**
 * @param {{ file: string, source: string }[]} files `file` is the path used in messages and to
 *   decide the kind; `source` the contents
 * @returns {string[]}
 */
export function checkTokens(files) {
  /** @type {string[]} */
  const problems = [];
  for (const { file, source } of files) {
    const posix = file.split(path.sep).join('/');
    if (EXEMPT.some((exempt) => posix.endsWith(exempt))) continue;
    const ext = path.extname(file);
    const seen = new Set();
    const report = (line, message) => {
      const entry = `${file}:${line}: ${message}`;
      if (!seen.has(entry)) {
        seen.add(entry);
        problems.push(entry);
      }
    };

    for (const match of source.matchAll(PALETTE_VAR))
      report(lineAt(source, match.index ?? 0), `\`${match[0]}\` is only allowed in tokens.css`);

    if (ext === '.css') {
      checkCssText(source, 0, source, report);
      continue;
    }

    if (ext === '.svg') {
      for (const match of source.matchAll(SVG_COLOR_ATTRIBUTE)) {
        if (!SVG_ALLOWED.test(match[3].trim()))
          report(
            lineAt(source, match.index ?? 0),
            `SVG \`${match[1]}="${match[3]}"\`; use currentColor or var(--…)`,
          );
      }
      continue;
    }

    // Utility classes anywhere in the file (class=, class:, cn(...), tv(...)).
    for (const match of source.matchAll(ARBITRARY_COLOR))
      report(
        lineAt(source, match.index ?? 0),
        `arbitrary color value \`${match[0]}…\`; use a token`,
      );
    for (const match of source.matchAll(RAW_PALETTE_CLASS))
      report(
        lineAt(source, match.index ?? 0),
        `raw palette class \`${match[0]}\`; use a semantic token`,
      );
    for (const match of source.matchAll(RING_OPACITY))
      report(
        lineAt(source, match.index ?? 0),
        `\`${match[0]}\`: the focus ring uses --ring at full opacity (D6)`,
      );

    for (const { text, index } of strings(source)) {
      if (OUTLINE_NONE.test(text) && !RING_REPLACEMENT.test(text))
        report(lineAt(source, index), '`outline-none` without a replacement focus ring (D6)');
    }

    if (ext === '.ts') {
      for (const { text, index } of strings(source)) {
        if (HEX.test(text) || COLOR_FUNCTION.test(text))
          report(
            lineAt(source, index),
            `color literal in a string (\`${text.slice(0, 40)}\`); use a token`,
          );
      }
      continue;
    }

    // .svelte
    for (const match of source.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)) {
      const start = (match.index ?? 0) + match[0].indexOf('>') + 1;
      checkCssText(match[1], start, source, report);
    }
    for (const match of source.matchAll(/\sstyle\s*=\s*(["'])(.*?)\1/gs)) {
      checkCssText(match[2], match.index ?? 0, source, report);
    }
    for (const match of source.matchAll(/\sstyle:([a-z-]+)\s*=\s*(["'])(.*?)\2/gis)) {
      checkCssText(`${match[1]}: ${match[3]}`, match.index ?? 0, source, report);
    }
    for (const match of source.matchAll(/\sstyle:([a-z-]+)\s*=\s*\{\s*(["'`])(.*?)\2\s*\}/gis)) {
      checkCssText(`${match[1]}: ${match[3]}`, match.index ?? 0, source, report);
    }
    for (const svg of source.matchAll(/<svg[\s\S]*?<\/svg>/g)) {
      for (const match of svg[0].matchAll(SVG_COLOR_ATTRIBUTE)) {
        if (!SVG_ALLOWED.test(match[3].trim()))
          report(
            lineAt(source, (svg.index ?? 0) + (match.index ?? 0)),
            `SVG \`${match[1]}="${match[3]}"\`; use currentColor or var(--…)`,
          );
      }
    }
    for (const match of source.matchAll(/(?<![\w-])data-theme\s*=/g))
      report(lineAt(source, match.index ?? 0), '`data-theme` only lives on <html> (D1)');
  }
  return problems;
}

/** All scannable files under `dir`, as `{ file, source }` with `file` relative to the repo. */
export function collectFiles(dir) {
  /** @type {{ file: string, source: string }[]} */
  const out = [];
  const walk = (current) => {
    for (const entry of readdirSync(current)) {
      const full = path.join(current, entry);
      if (statSync(full).isDirectory()) {
        if (!SKIP_DIRS.has(entry)) walk(full);
      } else if (EXTENSIONS.has(path.extname(entry))) {
        out.push({ file: path.relative(ROOT, full), source: readFileSync(full, 'utf8') });
      }
    }
  };
  walk(dir);
  return out;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2] ? path.resolve(process.argv[2]) : path.join(ROOT, 'apps/web/src');
  const problems = checkTokens(collectFiles(dir));
  if (problems.length > 0) {
    console.error(`check-tokens: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('check-tokens: ok');
}
