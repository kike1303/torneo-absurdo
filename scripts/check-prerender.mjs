#!/usr/bin/env node
// RF-A2 and ADR-0009 D3: no route prerenders (the theme and the CSP nonce are per request).
// Fails if a file under apps/web/src/routes declares `prerender = true` or `prerender = 'auto'`.
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const PRERENDER = /\bprerender\s*=\s*(true|(["'`])auto\2)/g;
const EXTENSIONS = new Set(['.ts', '.js', '.svelte']);

/**
 * @param {string} routesDir absolute path to a `src/routes` directory
 * @returns {string[]}
 */
export function checkPrerender(routesDir) {
  /** @type {string[]} */
  const problems = [];
  if (!existsSync(routesDir)) return problems;
  const walk = (dir) => {
    for (const entry of readdirSync(dir).sort()) {
      const full = path.join(dir, entry);
      if (statSync(full).isDirectory()) {
        walk(full);
        continue;
      }
      if (!EXTENSIONS.has(path.extname(entry))) continue;
      const source = readFileSync(full, 'utf8');
      for (const match of source.matchAll(PRERENDER)) {
        const line = source.slice(0, match.index ?? 0).split('\n').length;
        problems.push(
          `${path.relative(ROOT, full).split(path.sep).join('/')}:${line}: \`prerender = ${match[1]}\` is forbidden (ADR-0009 D3)`,
        );
      }
    }
  };
  walk(routesDir);
  return problems;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2]
    ? path.resolve(process.argv[2])
    : path.join(ROOT, 'apps/web/src/routes');
  const problems = checkPrerender(dir);
  if (problems.length > 0) {
    console.error(`check-prerender: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('check-prerender: ok');
}
