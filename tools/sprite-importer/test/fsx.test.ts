import { mkdirSync, symlinkSync, truncateSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { afterAll, describe, expect, it } from 'vitest';
import { listDir, readBounded } from '../src/fsx.ts';
import { cleanupTmp, tmp } from './support/tmp.ts';

afterAll(cleanupTmp);

// 007 plan-b decision 11: bounded reads that never follow a symlink.
describe('listDir', () => {
  it('lists every entry with its own kind, in byte order', () => {
    const dir = tmp();
    writeFileSync(path.join(dir, 'b.txt'), '');
    writeFileSync(path.join(dir, 'Z.txt'), '');
    mkdirSync(path.join(dir, 'a'));
    symlinkSync('b.txt', path.join(dir, 'link'));
    expect(listDir(dir)).toEqual([
      { name: 'Z.txt', kind: 'file' },
      { name: 'a', kind: 'dir' },
      { name: 'b.txt', kind: 'file' },
      { name: 'link', kind: 'symlink' },
    ]);
  });
});

describe('readBounded', () => {
  it('reads a file up to the cap, inclusive', () => {
    const dir = tmp();
    writeFileSync(path.join(dir, 'f'), 'abcd');
    const read = readBounded(path.join(dir, 'f'), 4);
    expect(read.ok && read.bytes.toString()).toBe('abcd');
    expect(readBounded(path.join(dir, 'f'), 3)).toEqual({ ok: false, reason: 'too_large' });
  });

  it('caps a large sparse file without reading it whole', () => {
    const dir = tmp();
    const file = path.join(dir, 'big');
    writeFileSync(file, '');
    truncateSync(file, 64 * 1024 * 1024);
    expect(readBounded(file, 1024)).toEqual({ ok: false, reason: 'too_large' });
  });

  it('never follows a symlink and tells missing files and directories apart', () => {
    const dir = tmp();
    writeFileSync(path.join(dir, 'f'), 'x');
    symlinkSync('f', path.join(dir, 'link'));
    mkdirSync(path.join(dir, 'd'));
    expect(readBounded(path.join(dir, 'link'), 10)).toEqual({ ok: false, reason: 'symlink' });
    expect(readBounded(path.join(dir, 'nope'), 10)).toEqual({ ok: false, reason: 'missing' });
    expect(readBounded(path.join(dir, 'd'), 10)).toEqual({ ok: false, reason: 'not_file' });
  });
});
