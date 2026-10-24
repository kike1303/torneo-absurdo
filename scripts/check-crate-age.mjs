#!/usr/bin/env node
// plan-b decision 16: every version in Cargo.lock, direct or transitive, must have been
// published on crates.io at least 3 days ago (the same cooldown Dependabot uses, RF-C16).
// cargo has no minimum release age, so this runs after `cargo generate-lockfile` and before
// the first `cargo build` (which runs build scripts and proc-macros), and on every lock change.
//
// Fails closed: a package from any source other than crates.io (git, another registry, a path
// that is not a workspace member) or a failed lookup makes the script exit with 1.
// Uses the network, so it is not part of `just check`; run it with `just crate-age`.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');

export const MIN_AGE_DAYS = 3;
export const WORKSPACE_MEMBERS = Object.freeze(['protocol', 'server']);
export const CRATES_IO_SOURCES = Object.freeze([
  'registry+https://github.com/rust-lang/crates.io-index',
  'sparse+https://index.crates.io/',
]);
const DAY_MS = 24 * 60 * 60 * 1000;
const USER_AGENT = 'torneo-absurdo-crate-age (https://github.com/kike1303/torneo-absurdo)';

/**
 * Minimal reader for the `[[package]]` tables of a Cargo.lock (v3/v4).
 * @param {string} text
 * @returns {{ name: string, version: string, source?: string }[]}
 */
export function parseCargoLock(text) {
  /** @type {{ name: string, version: string, source?: string }[]} */
  const packages = [];
  /** @type {Record<string, string> | null} */
  let current = null;
  const flush = () => {
    if (current === null) return;
    if (!current.name || !current.version) {
      throw new Error('Cargo.lock: [[package]] without name or version');
    }
    packages.push({
      name: current.name,
      version: current.version,
      ...(current.source !== undefined ? { source: current.source } : {}),
    });
    current = null;
  };
  for (const raw of text.split('\n')) {
    const line = raw.trim();
    if (line.startsWith('[')) {
      flush();
      if (line === '[[package]]') current = {};
      continue;
    }
    if (current === null) continue;
    const match = /^(name|version|source)\s*=\s*"([^"]*)"$/.exec(line);
    if (match) current[match[1]] = match[2];
  }
  flush();
  return packages;
}

/**
 * Splits the lock into crates.io packages to check and source problems.
 * @param {{ name: string, version: string, source?: string }[]} packages
 * @param {readonly string[]} [members]
 */
export function classifyPackages(packages, members = WORKSPACE_MEMBERS) {
  /** @type {{ name: string, version: string }[]} */
  const registry = [];
  /** @type {string[]} */
  const problems = [];
  for (const pkg of packages) {
    const id = `${pkg.name}@${pkg.version}`;
    if (pkg.source === undefined) {
      if (!members.includes(pkg.name)) {
        problems.push(`${id}: no source and not a workspace member (path dependency?)`);
      }
      continue;
    }
    if (!CRATES_IO_SOURCES.includes(pkg.source)) {
      problems.push(`${id}: source "${pkg.source}" is not crates.io`);
      continue;
    }
    registry.push({ name: pkg.name, version: pkg.version });
  }
  return { registry, problems };
}

/**
 * @param {string} createdAt RFC 3339 date from crates.io
 * @param {Date} now
 * @param {number} [minDays]
 */
export function isTooYoung(createdAt, now, minDays = MIN_AGE_DAYS) {
  const published = Date.parse(createdAt);
  if (Number.isNaN(published)) throw new Error(`invalid created_at "${createdAt}"`);
  return now.getTime() - published < minDays * DAY_MS;
}

/**
 * @param {object} options
 * @param {string} options.lockText contents of Cargo.lock
 * @param {(name: string, version: string) => Promise<string>} options.fetchCreatedAt
 * @param {Date} options.now
 * @param {() => Promise<void>} [options.pace] called between lookups (rate limit)
 * @param {readonly string[]} [options.members]
 */
export async function checkCrateAge({ lockText, fetchCreatedAt, now, pace, members }) {
  const { registry, problems } = classifyPackages(parseCargoLock(lockText), members);
  // Fail closed: an empty or truncated lock must never pass as "nothing too young".
  if (registry.length === 0)
    problems.push('no crates.io packages in Cargo.lock (empty or wrong file?)');
  /** @type {{ name: string, version: string, createdAt: string }[]} */
  const young = [];
  let checked = 0;
  for (const [index, pkg] of registry.entries()) {
    if (index > 0 && pace) await pace();
    let createdAt;
    try {
      createdAt = await fetchCreatedAt(pkg.name, pkg.version);
    } catch (error) {
      const reason = error instanceof Error ? error.message : 'unknown error';
      problems.push(`${pkg.name}@${pkg.version}: lookup failed (${reason})`);
      continue;
    }
    checked += 1;
    if (isTooYoung(createdAt, now)) young.push({ ...pkg, createdAt });
  }
  return { checked, young, problems };
}

/** @param {string} name @param {string} version */
async function fetchFromCratesIo(name, version) {
  const url = `https://crates.io/api/v1/crates/${encodeURIComponent(name)}/${encodeURIComponent(version)}`;
  const response = await fetch(url, {
    headers: { 'User-Agent': USER_AGENT, Accept: 'application/json' },
    signal: AbortSignal.timeout(20_000),
  });
  if (!response.ok) throw new Error(`HTTP ${response.status}`);
  const body = await response.json();
  const createdAt = body?.version?.created_at;
  if (typeof createdAt !== 'string') throw new Error('response without version.created_at');
  return createdAt;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const lockPath = path.resolve(process.argv[2] ?? path.join(ROOT, 'Cargo.lock'));
  const now = new Date();
  const result = await checkCrateAge({
    lockText: readFileSync(lockPath, 'utf8'),
    fetchCreatedAt: fetchFromCratesIo,
    now,
    // crates.io crawler policy: at most one request per second.
    pace: () => new Promise((resolve) => setTimeout(resolve, 1000)),
  });
  for (const pkg of result.young) {
    const days = ((now.getTime() - Date.parse(pkg.createdAt)) / DAY_MS).toFixed(1);
    console.error(
      `  too young: ${pkg.name}@${pkg.version} published ${pkg.createdAt} (${days} days)`,
    );
  }
  for (const problem of result.problems) console.error(`  ${problem}`);
  const summary = `check-crate-age: ${result.checked} crates.io package(s) checked, ${result.young.length} younger than ${MIN_AGE_DAYS} days, ${result.problems.length} problem(s)`;
  if (result.young.length > 0 || result.problems.length > 0) {
    console.error(summary);
    process.exit(1);
  }
  console.log(summary);
}
