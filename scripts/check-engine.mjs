#!/usr/bin/env node
// 003 RF5, RF19, RF20: keeps the engine pure and the WASM bindings minimal (`just _check-engine`,
// inside `check-rust`). Fails if:
// - the normal graph of `engine` alone (`cargo tree -p engine -e normal,features --target all`)
//   contains getrandom, rand, tokio, ts-rs, tracing or log, or rand_chacha / rand_core have the
//   serde, os_rng or std features. That is the resolution engine-wasm and the wasm target build;
//   in `cargo build --workspace` feature unification turns rand_core/os_rng on (tungstenite), and
//   crates/engine/clippy.toml forbids calling it.
// - crates/engine-wasm/src contains the token `unsafe`, or more than one `allow(unsafe_code)`;
// - the [lints] table of crates/engine-wasm differs from [workspace.lints] in anything other
//   than `unsafe_code = "deny"` (instead of "forbid").
// The functions are pure over text, so the tests feed them fixtures.
import { execFileSync } from 'node:child_process';
import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export const FORBIDDEN_CRATES = Object.freeze([
  'getrandom',
  'rand',
  'tokio',
  'ts-rs',
  'tracing',
  'log',
]);
export const RNG_CRATES = Object.freeze(['rand_chacha', 'rand_core']);
export const FORBIDDEN_RNG_FEATURES = Object.freeze(['serde', 'os_rng', 'std']);

/**
 * Reads the output of `cargo tree -e normal,features --prefix none`.
 * @param {string} text
 * @returns {{ packages: Set<string>, features: Map<string, Set<string>> }}
 */
export function parseCargoTree(text) {
  const packages = new Set();
  /** @type {Map<string, Set<string>>} */
  const features = new Map();
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (line === '') continue;
    const feature = /^([A-Za-z0-9_-]+) feature "([^"]+)"/.exec(line);
    if (feature) {
      const [, name, flag] = feature;
      if (!features.has(name)) features.set(name, new Set());
      features.get(name)?.add(flag);
      continue;
    }
    const pkg = /^([A-Za-z0-9_-]+) v\d/.exec(line);
    if (pkg) {
      packages.add(pkg[1]);
      continue;
    }
    throw new Error(`cargo tree: unexpected line: ${line}`);
  }
  return { packages, features };
}

/**
 * @param {{ packages: Set<string>, features: Map<string, Set<string>> }} tree
 * @returns {string[]} problems
 */
export function checkEngineGraph(tree) {
  /** @type {string[]} */
  const problems = [];
  if (!tree.packages.has('engine')) problems.push('engine: not in the cargo tree output');
  for (const name of FORBIDDEN_CRATES) {
    if (tree.packages.has(name)) problems.push(`${name}: in the normal graph of engine (RF5)`);
  }
  for (const name of RNG_CRATES) {
    for (const flag of FORBIDDEN_RNG_FEATURES) {
      if (tree.features.get(name)?.has(flag)) {
        problems.push(`${name}: feature "${flag}" is enabled in the graph of engine (RF5)`);
      }
    }
  }
  return problems;
}

const UNSAFE_TOKEN = /\bunsafe\b/g;
// Any allow/expect/warn attribute (outer `#[...]` or crate-level `#![...]`) whose lint list
// names unsafe_code, e.g. `#[allow(dead_code, unsafe_code)]` or `#![expect(unsafe_code)]`.
const RELAXING_ATTR = /#(!?)\s*\[\s*(allow|expect|warn)\s*\(([^)]*)\)\s*\]/g;
const UNSAFE_CODE = /(^|[\s,])unsafe_code([\s,]|$)/;

/**
 * Whether the item right after `offset` (skipping the rest of that line, blank lines, comments
 * and other outer attributes, one of which must be `#[wasm_bindgen]`) is a `pub fn`.
 * @param {string} text
 * @param {number} offset end of the attribute
 */
function isAboveExportedFn(text, offset) {
  const lines = text.slice(offset).split('\n').slice(1);
  let bindgen = false;
  for (const raw of lines) {
    const line = raw.trim();
    if (line === '' || line.startsWith('//')) continue;
    if (line.startsWith('#[')) {
      if (/^#\[\s*wasm_bindgen\b/.test(line)) bindgen = true;
      continue;
    }
    return bindgen && /^pub\s+fn\s+\w+/.test(line);
  }
  return false;
}

/**
 * @param {{ file: string, text: string }[]} sources the .rs files of crates/engine-wasm/src
 * @returns {string[]} problems
 */
export function checkWasmSources(sources) {
  /** @type {string[]} */
  const problems = [];
  /** @type {string[]} */
  const relaxing = [];
  for (const { file, text } of sources) {
    const tokens = text.match(UNSAFE_TOKEN)?.length ?? 0;
    if (tokens > 0)
      problems.push(`${file}: contains the token \`unsafe\` ${tokens} time(s) (RF19)`);
    for (const match of text.matchAll(RELAXING_ATTR)) {
      const [attr, inner, , list] = match;
      if (!UNSAFE_CODE.test(list)) continue;
      relaxing.push(`${file}: ${attr}`);
      if (inner === '!') {
        problems.push(`${file}: crate-level ${attr} (RF19: only on the exported function)`);
      } else if (!isAboveExportedFn(text, (match.index ?? 0) + attr.length)) {
        problems.push(`${file}: ${attr} is not right above the #[wasm_bindgen] pub fn (RF19)`);
      }
    }
  }
  if (relaxing.length > 1) {
    problems.push(
      `crates/engine-wasm/src: ${relaxing.length} attributes relax unsafe_code, at most 1 (RF19): ${relaxing.join('; ')}`,
    );
  }
  return problems;
}

/**
 * Reads the `[<prefix>.rust]` and `[<prefix>.clippy]` tables of a Cargo.toml (`key = "value"`
 * lines only, which is all the lint tables of this repo use).
 * @param {string} text
 * @param {string} prefix `workspace.lints` or `lints`
 * @returns {Record<string, Record<string, string>>} e.g. { rust: { unsafe_code: 'forbid' } }
 */
export function parseLintTables(text, prefix) {
  /** @type {Record<string, Record<string, string>>} */
  const tables = {};
  /** @type {Record<string, string> | null} */
  let current = null;
  for (const raw of text.split('\n')) {
    const line = raw.replace(/#.*$/, '').trim();
    if (line === '') continue;
    const header = /^\[([^\]]+)\]$/.exec(line);
    if (header) {
      const name = header[1].trim();
      const group = name.startsWith(`${prefix}.`) ? name.slice(prefix.length + 1) : null;
      if (group !== null) {
        tables[group] ??= {};
        current = tables[group];
      } else {
        current = null;
      }
      continue;
    }
    if (current === null) continue;
    const entry = /^([A-Za-z0-9_-]+)\s*=\s*(.+)$/.exec(line);
    if (!entry) throw new Error(`[${prefix}]: unexpected line: ${line}`);
    current[entry[1]] = entry[2].trim().replace(/^"(.*)"$/, '$1');
  }
  return tables;
}

/**
 * @param {string} workspaceToml the root Cargo.toml
 * @param {string} wasmToml crates/engine-wasm/Cargo.toml
 * @returns {string[]} problems
 */
export function checkWasmLints(workspaceToml, wasmToml) {
  const expected = parseLintTables(workspaceToml, 'workspace.lints');
  if (expected.rust?.unsafe_code !== 'forbid') {
    return ['Cargo.toml: [workspace.lints.rust] must have unsafe_code = "forbid"'];
  }
  expected.rust = { ...expected.rust, unsafe_code: 'deny' };
  const actual = parseLintTables(wasmToml, 'lints');
  /** @type {string[]} */
  const problems = [];
  for (const group of new Set([...Object.keys(expected), ...Object.keys(actual)])) {
    const want = expected[group] ?? {};
    const have = actual[group] ?? {};
    for (const key of new Set([...Object.keys(want), ...Object.keys(have)])) {
      if (want[key] !== have[key]) {
        problems.push(
          `crates/engine-wasm/Cargo.toml: [lints.${group}] ${key} = ${have[key] ?? '(missing)'}, expected ${want[key] ?? '(absent)'} (RF19)`,
        );
      }
    }
  }
  return problems;
}

/** @param {string} dir */
function rustSources(dir) {
  return readdirSync(dir, { recursive: true, withFileTypes: true })
    .filter((entry) => entry.isFile() && entry.name.endsWith('.rs'))
    .map((entry) => path.join(entry.parentPath, entry.name))
    .sort()
    .map((file) => ({ file: path.relative(ROOT, file), text: readFileSync(file, 'utf8') }));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const tree = execFileSync(
    'cargo',
    [
      'tree',
      '-p',
      'engine',
      '-e',
      'normal,features',
      '--target',
      'all',
      '--locked',
      '--prefix',
      'none',
    ],
    { cwd: ROOT, encoding: 'utf8' },
  );
  const read = (file) => readFileSync(path.join(ROOT, file), 'utf8');
  const problems = [
    ...checkEngineGraph(parseCargoTree(tree)),
    ...checkWasmSources(rustSources(path.join(ROOT, 'crates/engine-wasm/src'))),
    ...checkWasmLints(read('Cargo.toml'), read('crates/engine-wasm/Cargo.toml')),
  ];
  if (problems.length > 0) {
    console.error(`check-engine: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log('check-engine: ok');
}
