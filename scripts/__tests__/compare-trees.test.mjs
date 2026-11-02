import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it } from 'vitest';
import {
  GEN_FAILED_MESSAGE,
  STALE_MESSAGE,
  compareTrees,
  describeDifferences,
} from '../compare-trees.mjs';

/** @type {string[]} */
const temps = [];
const tree = (files) => {
  const dir = mkdtempSync(path.join(tmpdir(), 'compare-trees-'));
  temps.push(dir);
  for (const [name, content] of Object.entries(files)) {
    mkdirSync(path.dirname(path.join(dir, name)), { recursive: true });
    writeFileSync(path.join(dir, name), content);
  }
  return dir;
};
afterEach(() => {
  for (const dir of temps.splice(0)) rmSync(dir, { recursive: true, force: true });
});

const BASE = { 'index.ts': 'barrel', 'generated/A.ts': 'a', 'generated/B.ts': 'b' };

describe('compare-trees (plan-b decision 21)', () => {
  it('equal trees', () => {
    const result = compareTrees(tree(BASE), tree(BASE));
    expect(result.equal).toBe(true);
    expect(describeDifferences(result)).toEqual([]);
  });

  it('extra file before gen (a type deleted in Rust without regenerating)', () => {
    const result = compareTrees(tree(BASE), tree({ 'index.ts': 'barrel', 'generated/A.ts': 'a' }));
    expect(result).toMatchObject({
      equal: false,
      removed: ['generated/B.ts'],
      added: [],
      changed: [],
    });
    expect(describeDifferences(result)).toEqual([
      STALE_MESSAGE,
      '  removed by gen (stale file): generated/B.ts',
    ]);
  });

  it('missing file before gen (a new type not generated)', () => {
    const result = compareTrees(tree(BASE), tree({ ...BASE, 'generated/C.ts': 'c' }));
    expect(result).toMatchObject({ equal: false, removed: [], added: ['generated/C.ts'] });
    expect(describeDifferences(result)[1]).toBe('  missing before gen: generated/C.ts');
  });

  it('different contents', () => {
    const result = compareTrees(tree(BASE), tree({ ...BASE, 'generated/A.ts': 'a2' }));
    expect(result).toMatchObject({ equal: false, changed: ['generated/A.ts'] });
    expect(describeDifferences(result)).toEqual([
      STALE_MESSAGE,
      '  changed by gen: generated/A.ts',
    ]);
  });

  it('names the tree and its command, without saying it was regenerated (011 RF2)', () => {
    const result = compareTrees(tree(BASE), tree({ ...BASE, 'generated/A.ts': 'a2' }));
    const lines = describeDifferences(result, { name: 'engine-wasm', command: 'just gen-wasm' });
    expect(lines).toEqual([
      'engine-wasm is out of date: run just gen-wasm, review and commit',
      '  changed by gen: generated/A.ts',
    ]);
    expect(lines.join('\n')).not.toMatch(/regenerated|protocol-ts/);
    expect(
      describeDifferences(compareTrees(tree(BASE), tree(BASE)), { name: 'x', command: 'y' }),
    ).toEqual([]);
  });

  describe('command line', () => {
    const SCRIPT = fileURLToPath(new URL('../compare-trees.mjs', import.meta.url));
    const run = (...args) => spawnSync(process.execPath, [SCRIPT, ...args], { encoding: 'utf8' });

    it('with --name and --command, on a difference and on none', () => {
      const flags = ['--name', 'engine-wasm', '--command', 'just gen-wasm'];
      const stale = run(tree(BASE), tree({ ...BASE, 'generated/A.ts': 'a2' }), ...flags);
      expect(stale.status).toBe(1);
      expect(stale.stderr).toContain(
        'engine-wasm is out of date: run just gen-wasm, review and commit',
      );
      const fresh = run(tree(BASE), tree(BASE), ...flags);
      expect(fresh.status).toBe(0);
      expect(fresh.stdout).toContain('compare-trees: engine-wasm is up to date');
    });

    it('without them, the messages of _check-gen', () => {
      const stale = run(tree(BASE), tree({ ...BASE, 'generated/A.ts': 'a2' }));
      expect(stale.status).toBe(1);
      expect(stale.stderr).toContain(STALE_MESSAGE);
      expect(run(tree(BASE), tree(BASE)).stdout).toContain('protocol-ts is up to date');
    });

    it.each([[['--name', 'x']], [['--command', 'y']], [['--name']], [['--unknown', 'x']]])(
      'rejects %j: one flag alone would name the wrong command',
      (flags) => {
        expect(run(tree(BASE), tree(BASE), ...flags).status).toBe(2);
      },
    );
  });

  it('the two failure messages are distinct', () => {
    expect(GEN_FAILED_MESSAGE).toMatch(/^just gen failed/);
    expect(STALE_MESSAGE).toMatch(/^protocol-ts was out of date/);
  });
});
