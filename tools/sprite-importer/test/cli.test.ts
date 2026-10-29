import { spawnSync } from 'node:child_process';
import { existsSync, readdirSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';
import { SET, validRoot } from './support/fixture-root.ts';
import { EXPORT_REF } from './support/root.ts';
import { cleanupTmp, tmp } from './support/tmp.ts';

afterAll(cleanupTmp);

// 007 RF-B2, RF-B6 and plan-b decision 11: the real CLI, loaded by Node with type stripping (a
// failure to load the closure of @torneo/renderer/guide breaks this test, not only just check).
const PKG = fileURLToPath(new URL('..', import.meta.url));

function cli(args: string[], cwd: string = PKG) {
  const run = spawnSync(
    process.execPath,
    ['--experimental-strip-types', '--disable-warning=ExperimentalWarning', 'src/cli.ts', ...args],
    { cwd: PKG, env: { ...process.env, INIT_CWD: cwd }, encoding: 'utf8' },
  );
  const lines = run.stdout
    .split('\n')
    .filter((l) => l !== '')
    .map((l) => JSON.parse(l) as Record<string, unknown>);
  // No line, not even a usage error, names a temporary directory (plan-b decision 15).
  expect(run.stdout).not.toContain(os.tmpdir());
  expect(run.stdout).not.toContain('/private/');
  expect(run.stdout).not.toContain(path.basename(cwd));
  expect(run.stderr).toBe('');
  return { status: run.status, lines };
}

describe('cli check', () => {
  it('passes a valid root with 0 and a summary', () => {
    const root = validRoot();
    expect(cli(['check', '--', '--root', root])).toEqual({
      status: 0,
      lines: [{ sets: 1, characters: 1, links: 1 }],
    });
  });

  it('a relative --root resolves against INIT_CWD', () => {
    const root = validRoot();
    expect(cli(['check', '--root', path.basename(root)], path.dirname(root)).status).toBe(0);
  });

  it('fails a root with an error with 1 and JSON lines', () => {
    const root = validRoot();
    writeFileSync(path.join(root, ...SET, '.DS_Store'), '');
    expect(cli(['check', '--root', root])).toEqual({
      status: 1,
      lines: [{ code: 'set.unexpected_entry', file: 'content/sprites/ref_a/.DS_Store', path: '' }],
    });
  });

  it('a --root that is not a directory is a usage error', () => {
    expect(cli(['check', '--root', path.join(tmp(), 'nope')]).status).toBe(2);
  });
});

describe('cli usage', () => {
  it.each([
    ['an unknown flag', ['check', '--nope']],
    ['an unknown command', ['frobnicate']],
    ['no command', []],
    ['import without --in', ['import', '--out', 'content/_local/sprites/ref_a']],
    ['import without --out', ['import', '--in', EXPORT_REF]],
    ['import with --root', ['import', '--root', '.', '--in', EXPORT_REF, '--out', 'x']],
    ['import with an --in that is not a folder', ['import', '--in', 'nope', '--out', 'x']],
  ])('%s exits with 2 and one cli line', (_name, args) => {
    const run = cli(args, tmp());
    expect(run.status).toBe(2);
    expect(run.lines).toHaveLength(1);
    expect(String(run.lines[0]?.['code'])).toMatch(/^cli\./);
    expect(run.lines[0]?.['file']).toBe('');
  });

  it('import with --out outside the repository exits with 2 and creates nothing', () => {
    const elsewhere = tmp();
    const out = path.join(elsewhere, 'content', 'sprites', 'ref_a');
    expect(cli(['import', '--in', EXPORT_REF, '--out', out], elsewhere)).toEqual({
      status: 2,
      lines: [{ code: 'cli.out_forbidden', file: '', path: '' }],
    });
    expect(existsSync(out)).toBe(false);
    expect(readdirSync(elsewhere)).toEqual([]);
  });
});
