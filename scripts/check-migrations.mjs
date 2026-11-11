#!/usr/bin/env node
// TA-46: a change never deletes or renames a migration of apps/server that the base already has.
//
//   node scripts/check-migrations.mjs [base]
//
// The server migrates with set_ignore_missing(true) (TA-43), so at startup a migration deleted or
// renamed in the repo looks the same as one of a newer image: nothing but this check catches it.
// Adding migrations is fine. Compares the merge base of `base` (default origin/main) and HEAD; the
// CI job `policy` passes HEAD^1, the base of the PR merge commit or the previous commit of main.
// Any git error exits non-zero: the check fails closed.
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { cleanGitEnv } from './git-env.mjs';

export const MIGRATIONS_DIR = 'apps/server/migrations';

/**
 * The problems of a `git diff --name-status -z -M` limited to the migrations directory.
 * @param {string} output NUL-separated records: status, path, and a second path on R and C
 * @returns {string[]} problems, empty when no existing migration was deleted or renamed
 */
export function migrationProblems(output) {
  const fields = output.split('\0');
  if (fields.at(-1) === '') fields.pop();
  const problems = [];
  for (let i = 0; i < fields.length;) {
    const status = fields[i++];
    const from = fields[i++];
    const to = /^[RC]/.test(status) ? fields[i++] : undefined;
    if (from === undefined || (/^[RC]/.test(status) && to === undefined)) {
      throw new Error(`unexpected git diff output after status ${JSON.stringify(status)}`);
    }
    if (!from.startsWith(`${MIGRATIONS_DIR}/`)) continue;
    if (status === 'D') {
      problems.push(`${from}: deleted, an existing migration is never deleted`);
    } else if (status.startsWith('R')) {
      problems.push(`${from}: renamed to ${to}, an existing migration is never renamed`);
    }
  }
  return problems;
}

/**
 * @param {string} cwd a git checkout
 * @param {string} base a revision
 */
export function checkMigrations(cwd, base) {
  // Without GIT_* (TA-80): `cwd` names the repository, also under the pre-push hook.
  const git = (...args) => execFileSync('git', args, { cwd, encoding: 'utf8', env: cleanGitEnv() });
  const mergeBase = git('merge-base', base, 'HEAD').trim();
  const diff = git(
    'diff',
    '--name-status',
    '-z',
    '-M',
    '--no-ext-diff',
    mergeBase,
    'HEAD',
    '--',
    MIGRATIONS_DIR,
  );
  return migrationProblems(diff);
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
  const base = process.argv[2] ?? 'origin/main';
  let problems;
  try {
    problems = checkMigrations(root, base);
  } catch (error) {
    console.error(`check-migrations: ${error instanceof Error ? error.message : error}`);
    process.exit(2);
  }
  if (problems.length > 0) {
    for (const problem of problems) console.error(problem);
    console.error('check-migrations: add a new migration instead (TA-46)');
    process.exit(1);
  }
  console.log('check-migrations: ok');
}
