#!/usr/bin/env node
// RF-A10 and plan-a decision 17: Kit 3 config must never weaken or pin CSRF origin checks.
// Fails if apps/web/vite.config.ts mentions `trustedOrigins`, `checkOrigin` or `paths.origin`
// (or an `origin:` key inside `paths`), or if apps/web/svelte.config.js exists.
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

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
  problems.push(
    ...checkViteConfigSource(readFileSync(viteConfig, 'utf8'), path.relative(ROOT, viteConfig)),
  );
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
