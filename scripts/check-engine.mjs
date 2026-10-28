#!/usr/bin/env node
// 003 RF5, RF19, RF20: keeps the engine pure and the WASM bindings minimal (`just _check-engine`,
// inside `check-rust`). Fails if:
// - the normal graph of `engine` alone (`cargo tree -p engine -e normal,features --target all`)
//   contains getrandom, rand, tokio, ts-rs, tracing or log, or rand_chacha / rand_core have the
//   serde, os_rng or std features. That is the resolution engine-wasm and the wasm target build;
//   in `cargo build --workspace` feature unification turns rand_core/os_rng on (tungstenite), and
//   crates/engine/clippy.toml forbids calling it.
// - the normal graph of its non-server consumers (`cargo tree -p content -p engine-wasm -p sim
//   ...`) breaks the same rules (003 plan-b decision 13): a `features = ["std"]` on rand_chacha,
//   or a rand, getrandom, tokio, tracing or log pulled in by a consumer, would be unified into
//   the engine it builds, even if the graph of the engine alone stays clean. content stays
//   without logging for good: a caller that needs it logs itself (ADR-0012). The consumers come
//   from `cargo metadata`: every workspace member that depends on engine, directly or through
//   another member, except the server (so a new crate cannot be forgotten in a hand-written list).
// - the server depends on sim in any way (normal, build or dev): sim derives its seeds from a
//   public constant, so a fight seed derived with it would be predictable (ADR-0002).
// - crates/engine-wasm/src contains the token `unsafe`; mentions `unsafe_code` anywhere outside
//   comments and a `deny`/`forbid` lint list (so any allow/expect/warn, direct, crate-level,
//   inside cfg_attr or produced by a macro, fails closed); or contains `include!` or a `path`
//   attribute, which would pull in code this check does not read;
// - crates/engine-wasm/Cargo.toml does not inherit the workspace lints (`[lints] workspace =
//   true`, so unsafe_code = "forbid") or has a [lints.*] table of its own.
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
 * @param {{ roots?: readonly string[] }} [options] the packages the `cargo tree -p` was run
 *   for: each must be in the output, and the messages name them.
 * @returns {string[]} problems
 */
export function checkEngineGraph(tree, { roots = ['engine'] } = {}) {
  /** @type {string[]} */
  const problems = [];
  const graph = roots.join(' and ');
  for (const root of roots) {
    if (!tree.packages.has(root)) problems.push(`${root}: not in the cargo tree output`);
  }
  for (const name of FORBIDDEN_CRATES) {
    if (tree.packages.has(name)) problems.push(`${name}: in the normal graph of ${graph} (RF5)`);
  }
  for (const name of RNG_CRATES) {
    for (const flag of FORBIDDEN_RNG_FEATURES) {
      if (tree.features.get(name)?.has(flag)) {
        problems.push(`${name}: feature "${flag}" is enabled in the graph of ${graph} (RF5)`);
      }
    }
  }
  return problems;
}

/**
 * Workspace members that build the engine but are not checked as its consumers. The server
 * unifies rand_core/os_rng on through tungstenite, and crates/engine/clippy.toml forbids calling
 * it (see the header).
 */
export const NOT_ENGINE_CONSUMERS = Object.freeze(['server']);

/**
 * The engine's consumers outside the server (003 plan-b decision 13), from the output of
 * `cargo metadata --format-version 1 --no-deps`: every workspace member whose normal or build
 * dependencies reach `engine` through workspace members, sorted by name.
 * @param {{ packages: { name: string, dependencies: { name: string, kind: string | null, path?: string }[] }[] }} metadata
 * @returns {string[]}
 */
export function engineConsumers(metadata) {
  const members = new Map(metadata.packages.map((pkg) => [pkg.name, pkg]));
  if (!members.has('engine')) throw new Error('cargo metadata: engine is not a workspace member');
  /** @param {string} name */
  const deps = (name) =>
    (members.get(name)?.dependencies ?? [])
      .filter((dep) => dep.kind !== 'dev' && dep.path !== undefined && members.has(dep.name))
      .map((dep) => dep.name);
  /** @param {string} name */
  const reachesEngine = (name) => {
    const seen = new Set([name]);
    const queue = [...deps(name)];
    while (queue.length > 0) {
      const next = queue.pop() ?? '';
      if (next === 'engine') return true;
      if (seen.has(next)) continue;
      seen.add(next);
      queue.push(...deps(next));
    }
    return false;
  };
  return [...members.keys()]
    .filter((name) => name !== 'engine' && !NOT_ENGINE_CONSUMERS.includes(name))
    .filter(reachesEngine)
    .sort();
}

/** Workspace members the server must never depend on (ADR-0002: sim seeds are public). */
export const SERVER_FORBIDDEN_MEMBERS = Object.freeze(['sim']);

/**
 * @param {{ packages: Set<string> }} tree `cargo tree -p server -e normal,build,dev`
 * @returns {string[]} problems
 */
export function checkServerGraph(tree) {
  /** @type {string[]} */
  const problems = [];
  if (!tree.packages.has('server')) problems.push('server: not in the cargo tree output');
  for (const name of SERVER_FORBIDDEN_MEMBERS) {
    if (tree.packages.has(name)) {
      problems.push(`${name}: in the dependency graph of server (ADR-0002: its seeds are public)`);
    }
  }
  return problems;
}

function cargoMetadata() {
  return JSON.parse(
    execFileSync('cargo', ['metadata', '--format-version', '1', '--no-deps', '--locked'], {
      cwd: ROOT,
      encoding: 'utf8',
      maxBuffer: 64 * 1024 * 1024,
    }),
  );
}

/**
 * @param {string[]} packages
 * @param {string} [edges]
 */
function cargoTree(packages, edges = 'normal,features') {
  return execFileSync(
    'cargo',
    [
      'tree',
      ...packages.flatMap((name) => ['-p', name]),
      '-e',
      edges,
      '--target',
      'all',
      '--locked',
      '--prefix',
      'none',
    ],
    { cwd: ROOT, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 },
  );
}

const UNSAFE_TOKEN = /\bunsafe\b/g;
const UNSAFE_CODE = /\bunsafe_code\b/g;
// `deny(...)` / `forbid(...)` lint lists (only those may name unsafe_code), no nested parens.
const STRICT_LIST = /\b(deny|forbid)\s*\(([^()]*)\)/g;
const INCLUDE = /\binclude\s*!/g;
const PATH_ATTR = /#\s*!?\s*\[\s*path\b|\bcfg_attr\s*\([^\]]*\bpath\s*=/g;

/**
 * Replaces the comments of Rust source (`//...`, nested `/* ... *\/`) with spaces, keeping
 * newlines and string and char literals (a `//` inside a string is not a comment).
 * @param {string} text
 */
export function stripComments(text) {
  let out = '';
  let i = 0;
  const blank = (chunk) => chunk.replace(/[^\n]/g, ' ');
  while (i < text.length) {
    const rest = text.slice(i);
    if (rest.startsWith('//')) {
      const end = text.indexOf('\n', i);
      const stop = end === -1 ? text.length : end;
      out += blank(text.slice(i, stop));
      i = stop;
    } else if (rest.startsWith('/*')) {
      let depth = 0;
      let j = i;
      while (j < text.length) {
        if (text.startsWith('/*', j)) {
          depth += 1;
          j += 2;
        } else if (text.startsWith('*/', j)) {
          depth -= 1;
          j += 2;
          if (depth === 0) break;
        } else {
          j += 1;
        }
      }
      out += blank(text.slice(i, j));
      i = j;
    } else {
      const boundary = i === 0 || !/[A-Za-z0-9_]/.test(text[i - 1] ?? '');
      const raw = boundary ? /^b?r(#*)"/.exec(rest) : null;
      const literal = raw
        ? null
        : /^b?"(?:[^"\\]|\\[\s\S])*"|^b?'(?:[^'\\\n]|\\[^\n]{1,10})'/.exec(rest);
      if (raw) {
        const close = `"${raw[1]}`;
        const end = text.indexOf(close, i + raw[0].length);
        const stop = end === -1 ? text.length : end + close.length;
        out += text.slice(i, stop);
        i = stop;
      } else if (literal) {
        out += literal[0];
        i += literal[0].length;
      } else {
        out += text[i];
        i += 1;
      }
    }
  }
  return out;
}

/**
 * @param {{ file: string, text: string }[]} sources the .rs files of crates/engine-wasm/src
 * @returns {string[]} problems
 */
export function checkWasmSources(sources) {
  /** @type {string[]} */
  const problems = [];
  for (const { file, text } of sources) {
    const tokens = text.match(UNSAFE_TOKEN)?.length ?? 0;
    if (tokens > 0)
      problems.push(`${file}: contains the token \`unsafe\` ${tokens} time(s) (RF19)`);
    const code = stripComments(text);
    const relaxing = code.replace(STRICT_LIST, '').match(UNSAFE_CODE)?.length ?? 0;
    if (relaxing > 0) {
      problems.push(
        `${file}: mentions unsafe_code ${relaxing} time(s) outside a deny/forbid list (RF19: no allow, expect or warn, also via cfg_attr or a macro)`,
      );
    }
    if (INCLUDE.test(code)) problems.push(`${file}: uses include! (RF19)`);
    INCLUDE.lastIndex = 0;
    if (PATH_ATTR.test(code)) problems.push(`${file}: uses a #[path] attribute (RF19)`);
    PATH_ATTR.lastIndex = 0;
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
 * engine-wasm inherits the workspace lints (unsafe_code = "forbid") and has none of its own.
 * @param {string} workspaceToml the root Cargo.toml
 * @param {string} wasmToml crates/engine-wasm/Cargo.toml
 * @returns {string[]} problems
 */
export function checkWasmLints(workspaceToml, wasmToml) {
  /** @type {string[]} */
  const problems = [];
  if (parseLintTables(workspaceToml, 'workspace.lints').rust?.unsafe_code !== 'forbid') {
    problems.push('Cargo.toml: [workspace.lints.rust] must have unsafe_code = "forbid"');
  }
  let inLints = false;
  let inherits = false;
  for (const raw of wasmToml.split('\n')) {
    const line = raw.replace(/#.*$/, '').trim();
    if (line === '') continue;
    const header = /^\[([^\]]+)\]$/.exec(line);
    if (header) {
      const name = header[1].trim();
      inLints = name === 'lints';
      if (name.startsWith('lints.')) {
        problems.push(
          `crates/engine-wasm/Cargo.toml: own [${name}] table (RF19: inherit the workspace lints)`,
        );
      }
      continue;
    }
    if (/^lints\s*[.=]/.test(line)) {
      problems.push(`crates/engine-wasm/Cargo.toml: dotted lints key "${line}" (RF19)`);
    }
    if (inLints) {
      if (/^workspace\s*=\s*true$/.test(line)) inherits = true;
      else
        problems.push(
          `crates/engine-wasm/Cargo.toml: [lints] ${line} (RF19: only workspace = true)`,
        );
    }
  }
  if (!inherits) {
    problems.push('crates/engine-wasm/Cargo.toml: missing [lints] workspace = true (RF19)');
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
  const read = (file) => readFileSync(path.join(ROOT, file), 'utf8');
  const consumers = engineConsumers(cargoMetadata());
  const problems = [
    ...checkEngineGraph(parseCargoTree(cargoTree(['engine']))),
    ...(consumers.length > 0
      ? checkEngineGraph(parseCargoTree(cargoTree(consumers)), { roots: consumers })
      : []),
    ...checkServerGraph(parseCargoTree(cargoTree(['server'], 'normal,build,dev'))),
    ...checkWasmSources(rustSources(path.join(ROOT, 'crates/engine-wasm/src'))),
    ...checkWasmLints(read('Cargo.toml'), read('crates/engine-wasm/Cargo.toml')),
  ];
  if (problems.length > 0) {
    console.error(`check-engine: ${problems.length} problem(s)`);
    for (const p of problems) console.error(`  ${p}`);
    process.exit(1);
  }
  console.log(`check-engine: ok (engine consumers: ${consumers.join(', ')})`);
}
