import { cpSync, mkdtempSync, readFileSync, readdirSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import { HEADER, finalizeProtocolTs } from '../gen-protocol-ts.mjs';

const FIXTURES = fileURLToPath(new URL('../__fixtures__/gen-protocol-ts/', import.meta.url));
/** @type {string[]} */
const temps = [];
const copyFixture = (name) => {
  const dir = mkdtempSync(path.join(tmpdir(), 'gen-protocol-ts-'));
  temps.push(dir);
  cpSync(path.join(FIXTURES, name), dir, { recursive: true });
  return dir;
};
afterEach(() => {
  for (const dir of temps.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe('gen-protocol-ts (RF-B4, RF-B6)', () => {
  it('fails on bigint and names the file, without writing anything', () => {
    const dir = copyFixture('bigint');
    const { problems } = finalizeProtocolTs(dir);
    expect(problems).toEqual([expect.stringMatching(/^generated\/Counter\.ts: contains `bigint`/)]);
    expect(existsSync(path.join(dir, 'index.ts'))).toBe(false);
    expect(readFileSync(path.join(dir, 'generated/ApiErrorCode.ts'), 'utf8')).not.toContain(HEADER);
  });

  it('adds the header to every file and writes a sorted barrel of the files present', () => {
    const dir = copyFixture('valid');
    const { files, problems } = finalizeProtocolTs(dir);
    expect(problems).toEqual([]);
    expect(files).toEqual(['ApiErrorCode.ts', 'ErrorCode.ts']);
    for (const name of files) {
      expect(readFileSync(path.join(dir, 'generated', name), 'utf8').startsWith(HEADER)).toBe(true);
    }
    expect(readFileSync(path.join(dir, 'index.ts'), 'utf8')).toBe(
      HEADER +
        "export type * from './generated/ApiErrorCode.js';\n" +
        "export type * from './generated/ErrorCode.js';\n",
    );
  });

  it('is idempotent: running it twice does not duplicate the header', () => {
    const dir = copyFixture('valid');
    finalizeProtocolTs(dir);
    const first = readdirSync(path.join(dir, 'generated')).map((n) =>
      readFileSync(path.join(dir, 'generated', n), 'utf8'),
    );
    finalizeProtocolTs(dir);
    const second = readdirSync(path.join(dir, 'generated')).map((n) =>
      readFileSync(path.join(dir, 'generated', n), 'utf8'),
    );
    expect(second).toEqual(first);
  });

  it('the barrel only lists files that exist (a deleted type disappears)', () => {
    const dir = copyFixture('valid');
    rmSync(path.join(dir, 'generated/ErrorCode.ts'));
    finalizeProtocolTs(dir);
    expect(readFileSync(path.join(dir, 'index.ts'), 'utf8')).toBe(
      HEADER + "export type * from './generated/ApiErrorCode.js';\n",
    );
  });

  it('fails when ts-rs produced nothing', () => {
    const dir = mkdtempSync(path.join(tmpdir(), 'gen-protocol-ts-'));
    temps.push(dir);
    expect(finalizeProtocolTs(dir).problems).toEqual([
      expect.stringMatching(/directory not found/),
    ]);
  });
});
