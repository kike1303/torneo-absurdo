import { execFileSync, spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';
import { checkMigrations, MIGRATIONS_DIR, migrationProblems } from '../check-migrations.mjs';

// TA-46: runs in the CI job `policy` (scripts/vitest.policy.config.mjs) and in `just check`.
const SCRIPT = fileURLToPath(new URL('../check-migrations.mjs', import.meta.url));
// From a git hook (pre-push runs `just check`) git exports GIT_DIR and friends: the temporary
// repositories below would then be the real one, and `git init` would mark it bare and write the
// test identity into its config. Every git these tests start finds its repository from its cwd.
for (const key of Object.keys(process.env)) if (key.startsWith('GIT_')) delete process.env[key];
const work = mkdtempSync(path.join(tmpdir(), 'check-migrations-test-'));
afterAll(() => rmSync(work, { recursive: true, force: true }));

const M = (name) => `${MIGRATIONS_DIR}/${name}`;
const z = (...fields) => `${fields.join('\0')}\0`;

describe('migrationProblems', () => {
  it('passes on no change, additions and edits outside the directory', () => {
    expect(migrationProblems('')).toEqual([]);
    expect(migrationProblems(z('A', M('2_new.sql')))).toEqual([]);
    expect(migrationProblems(z('D', 'apps/server/src/db.rs'))).toEqual([]);
  });

  it('fails on a deleted migration', () => {
    expect(migrationProblems(z('D', M('1_base.sql')))).toEqual([
      `${M('1_base.sql')}: deleted, an existing migration is never deleted`,
    ]);
  });

  it('fails on a renamed migration, with or without similarity 100', () => {
    const problems = migrationProblems(
      z('R100', M('1_base.sql'), M('1_other.sql'), 'R087', M('2_a.sql'), M('3_a.sql')),
    );
    expect(problems).toHaveLength(2);
    expect(problems[0]).toMatch(/1_base\.sql: renamed to .*1_other\.sql/);
    expect(problems[1]).toMatch(/2_a\.sql: renamed to .*3_a\.sql/);
  });

  it('keeps parsing after a copy record', () => {
    expect(migrationProblems(z('C100', M('1_a.sql'), M('2_a.sql'), 'D', M('1_a.sql')))).toEqual([
      `${M('1_a.sql')}: deleted, an existing migration is never deleted`,
    ]);
  });

  it('fails closed on truncated output', () => {
    expect(() => migrationProblems(z('R100', M('1_a.sql')))).toThrow(/unexpected/);
    expect(() => migrationProblems('D')).toThrow(/unexpected/);
  });
});

let n = 0;
/** A git repository whose base commit has two migrations; `change` edits the tree for HEAD. */
function repo(change) {
  const dir = path.join(work, `repo-${n++}`);
  const git = (...args) => execFileSync('git', args, { cwd: dir, encoding: 'utf8' });
  mkdirSync(path.join(dir, MIGRATIONS_DIR), { recursive: true });
  git('init', '-q', '-b', 'main');
  git('config', 'user.email', 'test@example.invalid');
  git('config', 'user.name', 'test');
  git('config', 'commit.gpgsign', 'false');
  writeFileSync(path.join(dir, M('1_base.sql')), 'create table a (id int);\n');
  writeFileSync(path.join(dir, M('2_more.sql')), 'create table b (id int);\n');
  git('add', '-A');
  git('commit', '-q', '-m', 'base');
  git('switch', '-q', '-c', 'pr');
  change(dir);
  git('add', '-A');
  git('commit', '-q', '--allow-empty', '-m', 'change');
  return dir;
}

describe('checkMigrations', () => {
  it('passes when a migration is added', () => {
    const dir = repo((d) => writeFileSync(path.join(d, M('3_new.sql')), 'select 1;\n'));
    expect(checkMigrations(dir, 'main')).toEqual([]);
  });

  it('fails when a migration is deleted', () => {
    const dir = repo((d) => rmSync(path.join(d, M('2_more.sql'))));
    expect(checkMigrations(dir, 'main')).toEqual([
      `${M('2_more.sql')}: deleted, an existing migration is never deleted`,
    ]);
  });

  it('fails when a migration is renamed inside or out of the directory', () => {
    const inside = repo((d) =>
      renameSync(path.join(d, M('1_base.sql')), path.join(d, M('1_x.sql'))),
    );
    expect(checkMigrations(inside, 'main')).toEqual([
      `${M('1_base.sql')}: renamed to ${M('1_x.sql')}, an existing migration is never renamed`,
    ]);
    const outside = repo((d) => renameSync(path.join(d, M('1_base.sql')), path.join(d, 'x.sql')));
    expect(checkMigrations(outside, 'main')).toEqual([
      `${M('1_base.sql')}: deleted, an existing migration is never deleted`,
    ]);
  });

  it('compares against the merge base, not the tip of the base', () => {
    const dir = repo(() => {});
    const git = (...args) => execFileSync('git', args, { cwd: dir, encoding: 'utf8' });
    // A migration added on main after the branch point is not one the branch deleted.
    git('switch', '-q', 'main');
    writeFileSync(path.join(dir, M('3_main.sql')), 'select 1;\n');
    git('add', '-A');
    git('commit', '-q', '-m', 'main moves');
    git('switch', '-q', 'pr');
    expect(checkMigrations(dir, 'main')).toEqual([]);
  });

  it('exits 0, 1 or 2 from the command line', () => {
    // The script checks the repository it lives in: a copy of it inside each repo (built-ins only).
    const run = (dir, base) => {
      mkdirSync(path.join(dir, 'scripts'), { recursive: true });
      copyFileSync(SCRIPT, path.join(dir, 'scripts/check-migrations.mjs'));
      return spawnSync('node', ['scripts/check-migrations.mjs', base], {
        cwd: dir,
        encoding: 'utf8',
      });
    };
    expect(
      run(
        repo(() => {}),
        'main',
      ).status,
    ).toBe(0);
    const deleted = run(
      repo((d) => rmSync(path.join(d, M('1_base.sql')))),
      'main',
    );
    expect(deleted.status).toBe(1);
    expect(deleted.stderr).toMatch(/1_base\.sql: deleted/);
    expect(
      run(
        repo(() => {}),
        'no-such-revision-ta46',
      ).status,
    ).toBe(2);
  });
});
