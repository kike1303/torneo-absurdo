#!/usr/bin/env node
// plan-b decision 21: freshness of `just gen` without touching the git index. `_check-gen`
// copies packages/protocol-ts/src to a temporary directory, runs `just gen` in place and
// compares both trees with this script. A file present on one side only (for example the .ts
// of a type deleted in Rust) or with different contents means the committed tree was stale.
import { existsSync, readFileSync, readdirSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const GEN_FAILED_MESSAGE = 'just gen failed: the tree was not touched';
export const STALE_MESSAGE = 'protocol-ts was out of date: it was regenerated; review and commit';

/** @param {string} dir @returns {string[]} relative paths with `/` */
function listFiles(dir) {
  /** @type {string[]} */
  const out = [];
  if (!existsSync(dir)) return out;
  const walk = (current) => {
    for (const entry of readdirSync(current).sort()) {
      const full = path.join(current, entry);
      if (statSync(full).isDirectory()) walk(full);
      else out.push(path.relative(dir, full).split(path.sep).join('/'));
    }
  };
  walk(dir);
  return out;
}

/**
 * @param {string} before the tree as it was (committed)
 * @param {string} after the tree after `just gen`
 */
export function compareTrees(before, after) {
  const a = new Set(listFiles(before));
  const b = new Set(listFiles(after));
  const removed = [...a].filter((f) => !b.has(f)).sort();
  const added = [...b].filter((f) => !a.has(f)).sort();
  const changed = [...a]
    .filter((f) => b.has(f))
    .filter((f) => !readFileSync(path.join(before, f)).equals(readFileSync(path.join(after, f))))
    .sort();
  return { removed, added, changed, equal: removed.length + added.length + changed.length === 0 };
}

/** @param {ReturnType<typeof compareTrees>} result @returns {string[]} */
export function describeDifferences(result) {
  if (result.equal) return [];
  return [
    STALE_MESSAGE,
    ...result.removed.map((f) => `  removed by gen (stale file): ${f}`),
    ...result.added.map((f) => `  missing before gen: ${f}`),
    ...result.changed.map((f) => `  changed by gen: ${f}`),
  ];
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (process.argv[2] === '--gen-failed') {
    console.error(GEN_FAILED_MESSAGE);
    process.exit(1);
  }
  const [before, after] = process.argv.slice(2);
  if (!before || !after) {
    console.error('usage: compare-trees.mjs <before> <after> | --gen-failed');
    process.exit(2);
  }
  const lines = describeDifferences(compareTrees(path.resolve(before), path.resolve(after)));
  if (lines.length > 0) {
    for (const line of lines) console.error(line);
    process.exit(1);
  }
  console.log('compare-trees: protocol-ts is up to date');
}
