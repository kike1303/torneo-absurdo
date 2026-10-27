import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';
import { cleanGitEnv } from '../src/write.ts';
import { fixtureAir, fixtureSffV1, fixtureSffV2, MAP_JSON } from './support/fixtures.ts';
import { tempRepo, type TempRepo } from './support/git-repo.ts';

// RF-B2 and plan-b decision 11: the real CLI, loaded by Node with type stripping.
const PKG = fileURLToPath(new URL('..', import.meta.url));
let repo: TempRepo;

function cli(args: string[], extraEnv: Record<string, string> = {}) {
  const run = spawnSync(
    process.execPath,
    ['--experimental-strip-types', '--disable-warning=ExperimentalWarning', 'src/cli.ts', ...args],
    {
      cwd: PKG,
      env: { ...cleanGitEnv(process.env), INIT_CWD: repo.root, ...extraEnv },
      encoding: 'utf8',
    },
  );
  return { status: run.status, stdout: run.stdout, stderr: run.stderr };
}

beforeAll(() => {
  repo = tempRepo();
  mkdirSync(path.join(repo.root, 'ignored', 'in'), { recursive: true });
  writeFileSync(path.join(repo.root, 'ignored', 'in', 'c1.sff'), fixtureSffV1());
  writeFileSync(path.join(repo.root, 'ignored', 'in', 'c2.sff'), fixtureSffV2());
  writeFileSync(path.join(repo.root, 'ignored', 'in', 'c.air'), fixtureAir());
  writeFileSync(path.join(repo.root, 'ignored', 'in', 'map.json'), JSON.stringify(MAP_JSON));
  writeFileSync(path.join(repo.root, 'ignored', 'in', 'bad.json'), '{');
});
afterAll(() => repo.cleanup());

const inputs = (sff = 'c1.sff') => [
  '--sff',
  `ignored/in/${sff}`,
  '--air',
  'ignored/in/c.air',
  '--map',
  'ignored/in/map.json',
];

describe('cli', () => {
  it('convert: relative paths against INIT_CWD, a leading --, a JSON report without paths', () => {
    const run = cli(['convert', '--', ...inputs(), '--out', 'ignored/out/local_test']);
    expect(run.stderr).toBe('');
    expect(run.status).toBe(0);
    const report = JSON.parse(run.stdout);
    expect(report).toMatchObject({ id: 'local_test', sff_version: '1.01', frames: 15 });
    expect(run.stdout).not.toContain(path.basename(repo.root));
    expect(run.stdout).not.toContain('ignored/');
    expect(run.stdout).not.toContain('c1.sff');
    expect(existsSync(path.join(repo.root, 'ignored', 'out', 'local_test', 'manifest.json'))).toBe(
      true,
    );
  });

  it('convert of SFF v2', () => {
    const run = cli(['convert', ...inputs('c2.sff'), '--out', 'ignored/out/v2']);
    expect(run.status).toBe(0);
    expect(JSON.parse(run.stdout).sff_version).toBe('2.01');
  });

  it('a refusal to write is a code with exit 1', () => {
    const run = cli(['convert', ...inputs(), '--out', 'tracked/x']);
    expect(run.status).toBe(1);
    expect(JSON.parse(run.stderr)).toEqual({
      error: 'write.not_ignored',
      data: { file: 'manifest.json' },
    });
  });

  it('a missing input is cli.file_not_found with the argument and without the path', () => {
    const run = cli([
      'convert',
      '--sff',
      'ignored/in/nope.sff',
      '--air',
      'ignored/in/c.air',
      '--map',
      'ignored/in/map.json',
    ]);
    expect(run.status).toBe(1);
    expect(JSON.parse(run.stderr)).toEqual({ error: 'cli.file_not_found', data: { arg: 'sff' } });
    expect(run.stderr).not.toContain('nope');
  });

  it.skipIf(process.getuid?.() === 0)('an unreadable input is cli.file_unreadable', () => {
    const file = path.join(repo.root, 'ignored', 'in', 'locked.air');
    writeFileSync(file, fixtureAir());
    chmodSync(file, 0o000);
    const run = cli([
      'convert',
      '--sff',
      'ignored/in/c1.sff',
      '--air',
      'ignored/in/locked.air',
      '--map',
      'ignored/in/map.json',
    ]);
    chmodSync(file, 0o600);
    expect(JSON.parse(run.stderr)).toEqual({ error: 'cli.file_unreadable', data: { arg: 'air' } });
  });

  it('a directory is cli.not_a_file; invalid JSON is map.invalid_json; unknown flags are codes', () => {
    expect(
      JSON.parse(
        cli(['convert', '--sff', 'ignored', '--air', 'x', '--map', 'ignored/in/map.json']).stderr,
      ).error,
    ).toBe('cli.not_a_file');
    expect(
      JSON.parse(
        cli(['inspect', '--sff', 'ignored/in/c1.sff', '--map', 'ignored/in/bad.json']).stderr,
      ).error,
    ).toBe('map.invalid_json');
    const unknown = cli(['convert', '--nope', 'x']);
    expect(unknown.status).toBe(1);
    expect(JSON.parse(unknown.stderr).error).toBe('cli.bad_arguments');
    expect(JSON.parse(cli(['frobnicate']).stderr).error).toBe('cli.unknown_command');
  });

  it('inspect reads headers only and reports counts by compression and the map', () => {
    const run = cli(['inspect', '--', ...inputs('c2.sff')]);
    expect(run.status).toBe(0);
    expect(JSON.parse(run.stdout)).toEqual({
      sff_version: '2.01',
      sprites: 6,
      palettes: 1,
      compression: { raw: 1, rle8: 1, lz5: 1, png8: 1, png24: 1, png32: 1 },
      actions: 12,
      missing_actions: [],
      missing_sprites: 0,
      unsupported_sprites: [],
    });
  });

  it('inspect of SFF v1 without AIR', () => {
    const run = cli(['inspect', '--sff', 'ignored/in/c1.sff']);
    expect(JSON.parse(run.stdout)).toEqual({
      sff_version: '1.01',
      sprites: 6,
      palettes: 'individual',
      compression: { pcx: 6 },
    });
  });
});
