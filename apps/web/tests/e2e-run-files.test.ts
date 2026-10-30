import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { afterEach, describe, expect, it } from 'vitest';
import {
  ADMIN_CLI_LOG,
  DYNAMIC_CANARIES,
  EGRESS_LOG,
  EXPECTED_EGRESS,
  egressProblems,
} from '../e2e/support/logs.js';
import { RUN_FILES, STARTED_BY_THE_PROXY, resetRunFiles } from '../e2e/support/reset-run.mjs';

// 008 plan-d decision 10, condition 1 of devops (round 2): the files of the network guard and of
// the canaries fail closed. A leftover of an earlier local run never declares anything for this
// one, and a proxy that never started fails the teardown.
const dirs: string[] = [];
const tempDir = () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'e2e-run-'));
  dirs.push(dir);
  return dir;
};
afterEach(() => {
  for (const dir of dirs.splice(0)) rmSync(dir, { recursive: true, force: true });
});

describe('resetRunFiles (just _e2e-run, before Playwright)', () => {
  it('empties the declarations and the CLI log, and removes egress.log', () => {
    const dir = tempDir();
    for (const name of [...RUN_FILES, STARTED_BY_THE_PROXY]) {
      writeFileSync(path.join(dir, name), 'left over from another run\n');
    }
    resetRunFiles(dir);
    for (const name of RUN_FILES) expect(readFileSync(path.join(dir, name), 'utf8')).toBe('');
    // The proxy creates it again when it starts; if it never does, the teardown fails.
    expect(existsSync(path.join(dir, STARTED_BY_THE_PROXY))).toBe(false);
  });

  it('creates the directory and the files of a first run', () => {
    const dir = path.join(tempDir(), 'logs');
    resetRunFiles(dir);
    for (const name of RUN_FILES) expect(readFileSync(path.join(dir, name), 'utf8')).toBe('');
  });

  it('covers exactly the files that the specs and the teardown use', () => {
    expect([...RUN_FILES].sort()).toEqual(
      [EXPECTED_EGRESS, DYNAMIC_CANARIES, ADMIN_CLI_LOG].map((f) => path.basename(f)).sort(),
    );
    expect(STARTED_BY_THE_PROXY).toBe(path.basename(EGRESS_LOG));
  });

  it('runs in _e2e-run before playwright test', () => {
    const justfile = readFileSync(new URL('../../../justfile', import.meta.url), 'utf8');
    const recipe = /^_e2e-run:.*\n((?:[ \t]+.*\n|\n)*)/m.exec(justfile)?.[1] ?? '';
    const reset = recipe.indexOf('node apps/web/e2e/support/reset-run.mjs');
    const playwright = recipe.indexOf('playwright test');
    expect(reset).toBeGreaterThan(-1);
    expect(playwright).toBeGreaterThan(reset);
  });
});

describe('egressProblems (global teardown)', () => {
  const files = (egress: string | null, expected: string | null) => {
    const dir = tempDir();
    const log = path.join(dir, 'egress.log');
    const declared = path.join(dir, 'expected-egress.txt');
    if (egress !== null) writeFileSync(log, egress);
    if (expected !== null) writeFileSync(declared, expected);
    return [log, declared] as const;
  };

  it('fails when egress.log is missing (the proxy never started)', () => {
    expect(egressProblems(...files(null, 'example.com:443\n'))).toEqual([
      'egress: egress.log missing (did the proxy start?)',
    ]);
  });

  it('fails on a host that no test declared', () => {
    expect(
      egressProblems(...files('example.com:443\nevil.test:443\n', 'example.com:443\n')),
    ).toEqual(['egress: the browser tried to reach evil.test:443, which no test declared']);
  });

  it('fails on any host when nothing was declared', () => {
    expect(egressProblems(...files('example.com:443\n', null))).toHaveLength(1);
  });

  it('passes with an empty log or only declared hosts', () => {
    expect(egressProblems(...files('', null))).toEqual([]);
    expect(egressProblems(...files('example.com:443\n', 'example.com:443\n'))).toEqual([]);
  });
});
