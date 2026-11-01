import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterAll, describe, expect, it } from 'vitest';
import { ALWAYS, DEV_FIGHTS_STRINGS, DEV_STRINGS, checkBinary } from '../check-release-binary.mjs';

// 008 RF-D9, plan-d decision 12. Buffers built here: no binary is committed.
const ELF = Buffer.from([0x7f, 0x45, 0x4c, 0x46, 2, 1, 1, 0]);
const MACH_O = Buffer.from([0xcf, 0xfa, 0xed, 0xfe, 7, 0, 0, 1]);
const filler = Buffer.alloc(64, 0x2e);
const bin = (head, ...texts) =>
  Buffer.concat([head, filler, ...texts.map((t) => Buffer.from(`\0${t}\0`))]);

const release = bin(ELF, ALWAYS, '/api/health');
const dev = bin(ELF, ALWAYS, ...DEV_STRINGS, ...DEV_FIGHTS_STRINGS);

describe('checkBinary, release', () => {
  it('a release binary passes', () => {
    expect(checkBinary(release, { expectDev: false })).toEqual([]);
  });

  it.each(DEV_STRINGS)('%s fails', (text) => {
    const problems = checkBinary(bin(ELF, ALWAYS, text), { expectDev: false });
    expect(problems).toEqual([`contains ${text}: built with dev-auth`]);
  });

  it.each(DEV_FIGHTS_STRINGS)('%s fails', (text) => {
    const problems = checkBinary(bin(ELF, ALWAYS, text), { expectDev: false });
    expect(problems).toEqual([`contains ${text}: built with dev-fights`]);
  });

  it('without __Host-session fails (positive control)', () => {
    expect(checkBinary(bin(ELF, '/api/health'), { expectDev: false })[0]).toMatch(
      /does not contain __Host-session/,
    );
    expect(checkBinary(ELF, { expectDev: false })[0]).toMatch(/positive control/);
  });

  it('without the ELF header fails, even with __Host-session', () => {
    expect(checkBinary(Buffer.from(`#!/bin/sh\n${ALWAYS}`), { expectDev: false })).toEqual([
      'not an ELF executable',
    ]);
    expect(checkBinary(Buffer.from(''), { expectDev: false })).toEqual(['not an ELF executable']);
  });

  it('a Mach-O is never a release', () => {
    expect(checkBinary(bin(MACH_O, ALWAYS), { expectDev: false })).toEqual([
      'not an ELF executable',
    ]);
  });
});

describe('checkBinary, --expect-dev', () => {
  it('the binary with both features passes, ELF or Mach-O', () => {
    expect(checkBinary(dev, { expectDev: true })).toEqual([]);
    expect(
      checkBinary(bin(MACH_O, ALWAYS, ...DEV_STRINGS, ...DEV_FIGHTS_STRINGS), { expectDev: true }),
    ).toEqual([]);
  });

  it('a binary with dev-auth alone fails', () => {
    expect(checkBinary(bin(ELF, ALWAYS, ...DEV_STRINGS), { expectDev: true })).toEqual(
      DEV_FIGHTS_STRINGS.map((t) => `does not contain ${t}: not built with dev-fights`),
    );
  });

  it('a release binary fails, naming each missing string', () => {
    expect(checkBinary(release, { expectDev: true })).toEqual([
      ...DEV_STRINGS.map((t) => `does not contain ${t}: not built with dev-auth`),
      ...DEV_FIGHTS_STRINGS.map((t) => `does not contain ${t}: not built with dev-fights`),
    ]);
  });

  it('a script that names the strings fails', () => {
    expect(
      checkBinary(Buffer.from(`${ALWAYS} ${DEV_STRINGS.join(' ')}`), { expectDev: true }),
    ).toEqual(['not an ELF or Mach-O executable']);
  });
});

describe('the command line', () => {
  const dir = mkdtempSync(path.join(tmpdir(), 'release-binary-'));
  afterAll(() => rmSync(dir, { recursive: true, force: true }));
  const script = fileURLToPath(new URL('../check-release-binary.mjs', import.meta.url));
  const run = (...args) => spawnSync(process.execPath, [script, ...args], { encoding: 'utf8' });
  const write = (name, bytes) => {
    const file = path.join(dir, name);
    writeFileSync(file, bytes);
    return file;
  };

  it('exits 0 on success and 1 on a problem', () => {
    expect(run(write('release', release)).status).toBe(0);
    expect(run(write('dev', dev)).status).toBe(1);
    expect(run('--expect-dev', path.join(dir, 'dev')).status).toBe(0);
    expect(run('--expect-dev', path.join(dir, 'release')).status).toBe(1);
  });

  it('exits 2 on a usage error and 1 on a missing file', () => {
    expect(run().status).toBe(2);
    expect(run('--expect-dev').status).toBe(2);
    expect(run(path.join(dir, 'missing')).status).toBe(1);
  });
});
