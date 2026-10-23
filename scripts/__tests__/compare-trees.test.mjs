import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
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

  it('the two failure messages are distinct', () => {
    expect(GEN_FAILED_MESSAGE).toMatch(/^just gen failed/);
    expect(STALE_MESSAGE).toMatch(/^protocol-ts was out of date/);
  });
});
