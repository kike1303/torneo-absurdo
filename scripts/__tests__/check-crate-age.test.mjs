import { readFileSync } from 'node:fs';
import { describe, expect, it, vi } from 'vitest';
import {
  checkCrateAge,
  classifyPackages,
  isTooYoung,
  parseCargoLock,
  WORKSPACE_MEMBERS,
} from '../check-crate-age.mjs';

const fixture = (name) =>
  readFileSync(new URL(`../__fixtures__/check-crate-age/${name}.lock`, import.meta.url), 'utf8');
const NOW = new Date('2026-10-23T12:00:00Z');

describe('parseCargoLock', () => {
  it('reads name, version and source of every [[package]]', () => {
    expect(parseCargoLock(fixture('valid'))).toEqual([
      {
        name: 'itoa',
        version: '1.0.15',
        source: 'registry+https://github.com/rust-lang/crates.io-index',
      },
      { name: 'protocol', version: '0.1.0' },
      { name: 'serde', version: '1.0.229', source: 'sparse+https://index.crates.io/' },
      { name: 'server', version: '0.1.0' },
    ]);
  });
});

describe('classifyPackages (security, round 2, Minor 2)', () => {
  it('accepts workspace members without source and both crates.io index forms', () => {
    const { registry, problems } = classifyPackages(parseCargoLock(fixture('valid')));
    expect(problems).toEqual([]);
    expect(registry.map((p) => p.name)).toEqual(['itoa', 'serde']);
  });
  it('fails on a git source', () => {
    const { problems } = classifyPackages(parseCargoLock(fixture('git-source')));
    expect(problems).toEqual([expect.stringMatching(/^sneaky@0\.1\.0: source "git\+https/)]);
  });
  it('fails on a foreign registry', () => {
    const { problems } = classifyPackages(parseCargoLock(fixture('foreign-registry')));
    expect(problems).toEqual([expect.stringMatching(/^serde@1\.0\.229: source .* not crates\.io/)]);
  });
  it('fails on a package without source that is not a workspace member', () => {
    const { problems } = classifyPackages(parseCargoLock(fixture('path-dependency')));
    expect(problems).toEqual([expect.stringMatching(/^vendored@0\.1\.0: no source/)]);
  });
});

describe('isTooYoung', () => {
  it.each([
    ['2026-10-20T12:00:00Z', false], // exactly 3 days
    ['2026-10-20T12:00:01Z', true],
    ['2026-10-22T00:00:00Z', true],
    ['2025-01-01T00:00:00Z', false],
  ])('%s → %s', (createdAt, expected) => {
    expect(isTooYoung(createdAt, NOW)).toBe(expected);
  });
  it('throws on an invalid date', () => {
    expect(() => isTooYoung('nope', NOW)).toThrow();
  });
});

describe('checkCrateAge (network simulated)', () => {
  it('lists the versions younger than 3 days and checks only crates.io packages', async () => {
    const fetchCreatedAt = vi.fn(async (name) =>
      name === 'serde' ? '2026-10-22T08:00:00Z' : '2026-01-01T00:00:00Z',
    );
    const pace = vi.fn(async () => {});
    const result = await checkCrateAge({
      lockText: fixture('valid'),
      fetchCreatedAt,
      now: NOW,
      pace,
    });
    expect(fetchCreatedAt.mock.calls).toEqual([
      ['itoa', '1.0.15'],
      ['serde', '1.0.229'],
    ]);
    expect(pace).toHaveBeenCalledTimes(1);
    expect(result).toEqual({
      checked: 2,
      young: [{ name: 'serde', version: '1.0.229', createdAt: '2026-10-22T08:00:00Z' }],
      problems: [],
    });
  });
  it('passes when every version is old enough', async () => {
    const result = await checkCrateAge({
      lockText: fixture('valid'),
      fetchCreatedAt: async () => '2026-01-01T00:00:00Z',
      now: NOW,
    });
    expect(result.young).toEqual([]);
    expect(result.problems).toEqual([]);
  });
  it('reports a failed lookup as a problem (fails closed)', async () => {
    const result = await checkCrateAge({
      lockText: fixture('valid'),
      fetchCreatedAt: async (name) => {
        if (name === 'itoa') throw new Error('HTTP 503');
        return '2026-01-01T00:00:00Z';
      },
      now: NOW,
    });
    expect(result.problems).toEqual(['itoa@1.0.15: lookup failed (HTTP 503)']);
  });
});

describe('checkCrateAge fails closed (PR #3 review)', () => {
  it.each(['empty', 'workspace-only'])(
    'a lock without crates.io packages (%s) is a problem',
    async (name) => {
      const fetchCreatedAt = vi.fn(async () => '2026-01-01T00:00:00Z');
      const result = await checkCrateAge({ lockText: fixture(name), fetchCreatedAt, now: NOW });
      expect(fetchCreatedAt).not.toHaveBeenCalled();
      expect(result.checked).toBe(0);
      expect(result.problems).toEqual([
        expect.stringMatching(/no crates\.io packages in Cargo\.lock/),
      ]);
    },
  );
});

describe('WORKSPACE_MEMBERS (003 plan-a)', () => {
  it('is exactly the package names of the [workspace].members of Cargo.toml', () => {
    const root = new URL('../../', import.meta.url);
    const cargo = readFileSync(new URL('Cargo.toml', root), 'utf8');
    const members = /^members\s*=\s*\[([^\]]*)\]/m.exec(cargo)?.[1];
    expect(members).toBeDefined();
    const names = [...(members ?? '').matchAll(/"([^"]+)"/g)].map(([, dir]) => {
      const manifest = readFileSync(new URL(`${dir}/Cargo.toml`, root), 'utf8');
      const name = /^\[package\]\s*\nname\s*=\s*"([^"]+)"/m.exec(manifest)?.[1];
      if (!name) throw new Error(`${dir}/Cargo.toml has no [package] name`);
      return name;
    });
    expect([...WORKSPACE_MEMBERS].sort()).toEqual(names.sort());
  });
});
