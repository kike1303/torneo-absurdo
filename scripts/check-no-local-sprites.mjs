#!/usr/bin/env node
// 002 RF-B7 and plan-b decision 14: the dev-only serving of content/_local/sprites/ never
// reaches the build. Walks apps/web/build/ (every file, any type) and fails if one contains
// `__local-sprites` or `torneo:local-sprites`. Runs in `just e2e` right after the build.
// Source maps (`*.map`) are checked without their `sourcesContent`: that field is a copy of the
// original source, where the `import.meta.env.DEV` branch the build removed is still written.
import { existsSync, readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const BUILD_DIR = path.join(ROOT, 'apps/web/build');
export const NEEDLES = ['__local-sprites', 'torneo:local-sprites'];

/** @param {string} dir @returns {string[]} files relative to `dir`, sorted */
function walk(dir) {
  /** @type {string[]} */
  const out = [];
  const stack = [''];
  while (stack.length > 0) {
    const rel = stack.pop() ?? '';
    for (const entry of readdirSync(path.join(dir, rel), { withFileTypes: true })) {
      const child = rel === '' ? entry.name : `${rel}/${entry.name}`;
      if (entry.isDirectory()) stack.push(child);
      else if (entry.isFile()) out.push(child);
    }
  }
  return out.sort();
}

/** @param {string} text */
function withoutSourcesContent(text) {
  /** @type {unknown} */
  let map;
  try {
    map = JSON.parse(text);
  } catch {
    return text; // Not JSON: checked as is.
  }
  if (typeof map !== 'object' || map === null || Array.isArray(map)) return text;
  return JSON.stringify({ ...map, sourcesContent: undefined });
}

/**
 * @param {string} dir
 * @returns {{ files: number, problems: string[] }}
 */
export function checkNoLocalSprites(dir) {
  const files = walk(dir);
  const problems = [];
  for (const file of files) {
    let text = readFileSync(path.join(dir, file)).toString('latin1');
    if (file.endsWith('.map')) text = withoutSourcesContent(text);
    for (const needle of NEEDLES) {
      if (text.includes(needle)) problems.push(`${file}: contains ${needle}`);
    }
  }
  return { files: files.length, problems };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2] ? path.resolve(process.argv[2]) : BUILD_DIR;
  if (!existsSync(dir)) {
    console.error(`check-no-local-sprites: missing ${path.relative(ROOT, dir)} (run the build)`);
    process.exit(1);
  }
  const { files, problems } = checkNoLocalSprites(dir);
  if (files === 0) {
    console.error('check-no-local-sprites: the build is empty');
    process.exit(1);
  }
  if (problems.length > 0) {
    console.error(`check-no-local-sprites: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log(`check-no-local-sprites: ok (${files} files)`);
}
