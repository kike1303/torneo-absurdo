#!/usr/bin/env node
// 002 RF-A17 and plan-a2 decision 9: Pixi and the renderer only load through `import()`.
// Reads the client build of apps/web (`.svelte-kit/output/client/.vite/`): `manifest.json` (the
// chunk graph, forced by Kit) and `chunk-modules.json` (the modules inside each chunk, written by
// apps/web/vite/chunk-modules.ts, because the manifest does not list them). Fails if:
// - any entry or route node (the dev/arena one included) reaches a module of `pixi.js` or of
//   `packages/renderer/` through static imports;
// - no node holds `src/routes/dev/arena/+page.svelte`, or that node does not reach `pixi.js`
//   through dynamic imports (so the check never passes empty if the format changes).
// Prints the gzip size of the deferred closure of dev/arena as a report.
import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const BUILD_DIR = path.join(ROOT, 'apps/web/.svelte-kit/output/client');

const PIXI = /(?:^|\/)node_modules\/(?:\.pnpm\/[^/]+\/node_modules\/)?pixi\.js\//;
const RENDERER = /(?:^|\/)packages\/renderer\//;
const ARENA_PAGE = /(?:^|\/)src\/routes\/dev\/arena\/\+page\.svelte$/;

/** @typedef {{ file: string, src?: string, isEntry?: boolean, imports?: string[], dynamicImports?: string[] }} Chunk */

/**
 * @param {Record<string, Chunk>} manifest
 * @param {Iterable<string>} start
 * @param {boolean} dynamic follow `dynamicImports` too
 */
function closure(manifest, start, dynamic) {
  const seen = new Set();
  const stack = [...start];
  while (stack.length > 0) {
    const key = stack.pop();
    if (key === undefined || seen.has(key) || manifest[key] === undefined) continue;
    seen.add(key);
    const chunk = manifest[key];
    stack.push(...(chunk.imports ?? []));
    if (dynamic) stack.push(...(chunk.dynamicImports ?? []));
  }
  return seen;
}

/**
 * Modules of a set of manifest keys: the `src` of each and the modules of its chunk file.
 * @param {Record<string, Chunk>} manifest
 * @param {Record<string, string[]>} chunkModules
 * @param {Iterable<string>} keys
 */
function modulesOf(manifest, chunkModules, keys) {
  const modules = new Set();
  for (const key of keys) {
    const chunk = manifest[key];
    if (chunk === undefined) continue;
    if (chunk.src) modules.add(chunk.src);
    for (const id of chunkModules[chunk.file] ?? []) modules.add(id);
  }
  return modules;
}

const forbidden = (id) => PIXI.test(id) || RENDERER.test(id);

/**
 * @param {Record<string, Chunk>} manifest
 * @param {Record<string, string[]>} chunkModules
 * @returns {{ problems: string[], deferredFiles: string[] }}
 */
export function checkRendererSplit(manifest, chunkModules) {
  /** @type {string[]} */
  const problems = [];
  const entries = Object.keys(manifest)
    .filter((key) => manifest[key]?.isEntry)
    .sort();
  if (entries.length === 0) problems.push('manifest: no entries (did the format change?)');

  /** @type {string | undefined} */
  let arena;
  const staticFiles = new Set();
  for (const key of entries) {
    const reach = closure(manifest, [key], false);
    for (const k of reach) staticFiles.add(manifest[k]?.file);
    const modules = modulesOf(manifest, chunkModules, reach);
    const bad = [...modules].filter(forbidden).sort();
    if (bad.length > 0) {
      problems.push(
        `${key}: reaches ${bad[0]}${bad.length > 1 ? ` (+${bad.length - 1} more)` : ''} through static imports`,
      );
    }
    if ([...modulesOf(manifest, chunkModules, [key])].some((id) => ARENA_PAGE.test(id))) {
      arena = key;
    }
  }

  /** @type {string[]} */
  let deferredFiles = [];
  if (arena === undefined) {
    problems.push('no route node holds src/routes/dev/arena/+page.svelte');
  } else {
    const reach = closure(manifest, [arena], true);
    const own = modulesOf(manifest, chunkModules, closure(manifest, [arena], false));
    // Only what the dynamic edges add counts: Pixi reached statically is not the positive case.
    const deferred = [...modulesOf(manifest, chunkModules, reach)].filter((id) => !own.has(id));
    if (!deferred.some((id) => PIXI.test(id))) {
      problems.push(`${arena}: does not reach pixi.js through dynamic imports`);
    }
    deferredFiles = [...new Set([...reach].map((k) => manifest[k]?.file))]
      .filter((file) => file !== undefined && !staticFiles.has(file) && file.endsWith('.js'))
      .sort();
  }
  return { problems, deferredFiles };
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const dir = process.argv[2] ? path.resolve(process.argv[2]) : BUILD_DIR;
  const manifestFile = path.join(dir, '.vite/manifest.json');
  const modulesFile = path.join(dir, '.vite/chunk-modules.json');
  for (const file of [manifestFile, modulesFile]) {
    if (!existsSync(file)) {
      console.error(`check-renderer-split: missing ${path.relative(ROOT, file)} (run the build)`);
      process.exit(1);
    }
  }
  const { problems, deferredFiles } = checkRendererSplit(
    JSON.parse(readFileSync(manifestFile, 'utf8')),
    JSON.parse(readFileSync(modulesFile, 'utf8')),
  );
  if (problems.length > 0) {
    console.error(`check-renderer-split: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  let gzip = 0;
  for (const file of deferredFiles) {
    const full = path.join(dir, file);
    if (existsSync(full)) gzip += gzipSync(readFileSync(full)).length;
  }
  console.log(
    `check-renderer-split: ok (deferred closure of dev/arena: ${deferredFiles.length} chunks, ${(gzip / 1024).toFixed(1)} KiB gzip)`,
  );
}
