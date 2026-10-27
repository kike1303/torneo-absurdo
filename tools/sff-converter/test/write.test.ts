import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from 'node:fs';
import path from 'node:path';
import { PNG } from 'pngjs';
import { validateManifest } from '@torneo/renderer/manifest';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { convert } from '../src/convert.ts';
import { OUTPUT_MARKER, writeOutput } from '../src/write.ts';
import { code } from './support/expect-error.ts';
import { fixtureAir, fixtureMap, fixtureSffV1 } from './support/fixtures.ts';
import { tempDir, tempRepo, type TempRepo } from './support/git-repo.ts';

// RF-B5, plan-b decision 12.
const result = convert({ sff: fixtureSffV1(), air: fixtureAir(), map: fixtureMap() });
if (!result.ok) throw new Error('fixture does not convert');
const { manifest, sheets } = result;

let repo: TempRepo;
beforeEach(() => {
  repo = tempRepo();
});
afterEach(() => repo.cleanup());

describe('writeOutput', () => {
  it('writes into a directory the .gitignore ignores, and replaces old output only', () => {
    const out = path.join(repo.root, 'ignored', 'sprites', 'local_test');
    mkdirSync(out, { recursive: true });
    writeFileSync(path.join(out, OUTPUT_MARKER), '');
    writeFileSync(path.join(out, 'sheet-7.png'), 'old');
    writeFileSync(path.join(out, 'notes.txt'), 'keep');
    writeOutput(out, manifest, sheets);
    expect(readdirSync(out).sort()).toEqual([
      OUTPUT_MARKER,
      'manifest.json',
      'notes.txt',
      'sheet-0.png',
    ]);
    const json = JSON.parse(readFileSync(path.join(out, 'manifest.json'), 'utf8'));
    expect(validateManifest(json, { expectedId: 'local_test' }).ok).toBe(true);
    const png = PNG.sync.read(readFileSync(path.join(out, 'sheet-0.png')));
    expect([png.width, png.height]).toEqual([sheets[0]!.width, sheets[0]!.height]);
    expect(Buffer.from(png.data).equals(Buffer.from(sheets[0]!.rgba))).toBe(true);
  });

  // plan-c decision 9 (b): old output is only deleted next to the marker.
  it('refuses an ignored folder with a foreign manifest.json and leaves it intact', () => {
    const out = path.join(repo.root, 'ignored', 'foreign');
    mkdirSync(out, { recursive: true });
    const foreign = Buffer.from('{"not":"ours"}\n');
    writeFileSync(path.join(out, 'manifest.json'), foreign);
    expect(code(() => writeOutput(out, manifest, sheets))).toBe('write.foreign_output');
    expect(readdirSync(out)).toEqual(['manifest.json']);
    expect(readFileSync(path.join(out, 'manifest.json')).equals(foreign)).toBe(true);
  });

  it('with the marker, cleans the old output and writes again', () => {
    const out = path.join(repo.root, 'ignored', 'ours');
    writeOutput(out, manifest, sheets);
    writeFileSync(path.join(out, 'sheet-9.png'), 'stale');
    writeOutput(out, manifest, sheets);
    expect(readdirSync(out).sort()).toEqual([OUTPUT_MARKER, 'manifest.json', 'sheet-0.png']);
  });

  it('a new folder ends up with an empty marker', () => {
    const out = path.join(repo.root, 'ignored', 'new', 'x');
    writeOutput(out, manifest, sheets);
    expect(readFileSync(path.join(out, OUTPUT_MARKER)).length).toBe(0);
  });

  it('a marker that is a symbolic link is write.symlink, and nothing is deleted', () => {
    const out = path.join(repo.root, 'ignored', 'linked');
    mkdirSync(out, { recursive: true });
    writeFileSync(path.join(repo.root, 'ignored', 'elsewhere'), '');
    symlinkSync(path.join(repo.root, 'ignored', 'elsewhere'), path.join(out, OUTPUT_MARKER));
    writeFileSync(path.join(out, 'manifest.json'), 'old');
    expect(code(() => writeOutput(out, manifest, sheets))).toBe('write.symlink');
    expect(readFileSync(path.join(out, 'manifest.json'), 'utf8')).toBe('old');
  });

  it('a marker that is a directory is write.marker_invalid, and nothing is deleted', () => {
    const out = path.join(repo.root, 'ignored', 'dir-marker');
    mkdirSync(path.join(out, OUTPUT_MARKER), { recursive: true });
    writeFileSync(path.join(out, 'manifest.json'), 'old');
    expect(code(() => writeOutput(out, manifest, sheets))).toBe('write.marker_invalid');
    expect(readFileSync(path.join(out, 'manifest.json'), 'utf8')).toBe('old');
  });

  it('refuses when a `!` rule brings back the marker', () => {
    const local = tempRepo(`ignored/*\n!ignored/x/\nignored/x/*\n!ignored/x/${OUTPUT_MARKER}\n`);
    try {
      expect(code(() => writeOutput(path.join(local.root, 'ignored', 'x'), manifest, sheets))).toBe(
        'write.not_ignored',
      );
      expect(existsSync(path.join(local.root, 'ignored', 'x', OUTPUT_MARKER))).toBe(false);
    } finally {
      local.cleanup();
    }
  });

  it('refuses a directory git does not ignore, and leaves no directory behind', () => {
    const out = path.join(repo.root, 'tracked', 'a', 'b');
    expect(code(() => writeOutput(out, manifest, sheets))).toBe('write.not_ignored');
    expect(existsSync(path.join(repo.root, 'tracked', 'a'))).toBe(false);
  });

  it('refuses when a `!` rule brings back one of the files', () => {
    const local = tempRepo('ignored/*\n!ignored/x/\nignored/x/*\n!ignored/x/sheet-0.png\n');
    try {
      expect(code(() => writeOutput(path.join(local.root, 'ignored', 'x'), manifest, sheets))).toBe(
        'write.not_ignored',
      );
    } finally {
      local.cleanup();
    }
  });

  it('refuses the output as a symbolic link, and a symbolic link as nearest existing ancestor', () => {
    const target = path.join(repo.root, 'ignored', 'real');
    mkdirSync(target, { recursive: true });
    const link = path.join(repo.root, 'ignored', 'link');
    symlinkSync(target, link);
    expect(code(() => writeOutput(link, manifest, sheets))).toBe('write.symlink');
    expect(code(() => writeOutput(path.join(link, 'deeper', 'x'), manifest, sheets))).toBe(
      'write.symlink',
    );
    expect(readdirSync(target)).toEqual([]);
  });

  it('refuses when the nearest existing ancestor is a file', () => {
    mkdirSync(path.join(repo.root, 'ignored'));
    writeFileSync(path.join(repo.root, 'ignored', 'file'), '');
    expect(
      code(() => writeOutput(path.join(repo.root, 'ignored', 'file', 'x'), manifest, sheets)),
    ).toBe('write.not_a_directory');
  });

  it('outside a repository it is write.git_unavailable, and nothing is created', () => {
    const outside = tempDir();
    try {
      const out = path.join(outside, 'x');
      expect(code(() => writeOutput(out, manifest, sheets))).toBe('write.git_unavailable');
      expect(existsSync(out)).toBe(false);
    } finally {
      rmSync(outside, { recursive: true, force: true });
    }
  });

  it('without git in PATH it is write.git_unavailable, and nothing is created', () => {
    const out = path.join(repo.root, 'ignored', 'x');
    expect(code(() => writeOutput(out, manifest, sheets, { env: { PATH: '/nonexistent' } }))).toBe(
      'write.git_unavailable',
    );
    expect(existsSync(path.join(repo.root, 'ignored'))).toBe(false);
  });

  it('a GIT_DIR pointing at another repository does not change the answer', () => {
    const other = tempRepo('*\n');
    try {
      const env = {
        ...process.env,
        GIT_DIR: path.join(other.root, '.git'),
        GIT_WORK_TREE: other.root,
      };
      expect(
        code(() => writeOutput(path.join(repo.root, 'tracked', 'x'), manifest, sheets, { env })),
      ).toBe('write.not_ignored');
      writeOutput(path.join(repo.root, 'ignored', 'x'), manifest, sheets, { env });
      expect(existsSync(path.join(repo.root, 'ignored', 'x', 'manifest.json'))).toBe(true);
    } finally {
      other.cleanup();
    }
  });

  it('refuses an invalid manifest or sheets that do not match it', () => {
    const out = path.join(repo.root, 'ignored', 'x');
    expect(code(() => writeOutput(out, { ...manifest, id: 'Bad' }, sheets))).toBe(
      'write.invalid_manifest',
    );
    expect(code(() => writeOutput(out, manifest, []))).toBe('write.sheet_mismatch');
  });

  it('write.ts has no try/catch (plan-b decision 3)', () => {
    const source = readFileSync(new URL('../src/write.ts', import.meta.url), 'utf8');
    expect(source.replace(/\/\/.*$/gm, '')).not.toMatch(/\bcatch\b/);
  });
});
