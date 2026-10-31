import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  a9Parameters,
  ageSession,
  assertA9Apply,
  assertE2eTarget,
  maintenanceUrl,
  reset,
  userId,
  seedOauthAccount,
} from '../e2e/support/db.mjs';

// 008 plan-d decision 10: the guards of the e2e database (change 6 of devops, 5 of architect).
const PASSWORD = 'canary-pw-61d0';
const url = (host: string, db: string) => `postgres://torneo:${PASSWORD}@${host}:5432/${db}`;

function message(fn: () => unknown): string {
  try {
    fn();
  } catch (e) {
    return (e as Error).message;
  }
  throw new Error('expected a throw');
}

describe('assertE2eTarget', () => {
  it.each([
    url('127.0.0.1', 'torneo_e2e'),
    url('localhost', 'torneo_e2e'),
    url('[::1]', 'torneo_e2e'),
    url('postgres', 'torneo_e2e'),
    `postgresql://torneo:${PASSWORD}@postgres/torneo_e2e`,
  ])('accepts %s', (value) => expect(assertE2eTarget(value).pathname).toBe('/torneo_e2e'));

  it.each([
    [url('127.0.0.1', 'torneo'), /exactly the database torneo_e2e/],
    [url('127.0.0.1', 'torneo_e2e_x'), /exactly the database torneo_e2e/],
    [url('127.0.0.1', 'torneo_test_1'), /exactly the database torneo_e2e/],
    [url('127.0.0.1', ''), /exactly the database torneo_e2e/],
    [url('10.0.0.5', 'torneo_e2e'), /must point at one of/],
    [url('db.example.com', 'torneo_e2e'), /must point at one of/],
    [url('127.0.0.2', 'torneo_e2e'), /must point at one of/],
    [`mysql://torneo:${PASSWORD}@127.0.0.1/torneo_e2e`, /postgres:\/\//],
    ['not a url', /not a URL/],
    [undefined, /required/],
  ])('refuses %s, without the password', (value, rule) => {
    const text = message(() => assertE2eTarget(value));
    expect(text).toMatch(rule);
    expect(text).not.toContain(PASSWORD);
    expect(text).not.toContain('example.com');
  });

  // Security review of PR #56: sqlx reads `host`, `hostaddr` and `dbname` from the query and
  // postgres.js reads `database`, so a query would retarget a URL that passes the other rules.
  it.each([
    `${url('127.0.0.1', 'torneo_e2e')}?host=db.example.com`,
    `${url('127.0.0.1', 'torneo_e2e')}?hostaddr=10.0.0.5`,
    `${url('127.0.0.1', 'torneo_e2e')}?dbname=torneo`,
    `${url('127.0.0.1', 'torneo_e2e')}?database=torneo`,
    `${url('127.0.0.1', 'torneo_e2e')}?sslmode=disable`,
    `${url('127.0.0.1', 'torneo_e2e')}?`,
    `${url('postgres', 'torneo_e2e')}#db.example.com`,
    `${url('postgres', 'torneo_e2e')}#`,
  ])('refuses a query or fragment: %s, without the URL', (value) => {
    const text = message(() => assertE2eTarget(value));
    expect(text).toMatch(/no query string or fragment/);
    expect(text).not.toContain(PASSWORD);
    expect(text).not.toContain('example.com');
    expect(text).not.toContain('10.0.0.5');
  });

  it('a helper refuses a query before connecting', async () => {
    await expect(reset(`${url('127.0.0.1', 'torneo_e2e')}?host=db.example.com`)).rejects.toThrow(
      /no query string or fragment/,
    );
  });

  it('reset and the helpers refuse before connecting', async () => {
    await expect(reset(url('db.example.com', 'torneo_e2e'))).rejects.toThrow(/must point at/);
    await expect(reset(url('127.0.0.1', 'torneo'))).rejects.toThrow(/torneo_e2e/);
    await expect(userId('x', url('127.0.0.1', 'torneo'))).rejects.toThrow(/torneo_e2e/);
    await expect(seedOauthAccount('x', 'google', url('10.0.0.5', 'torneo_e2e'))).rejects.toThrow(
      /must point at/,
    );
    await expect(ageSession('x', 11, url('10.0.0.5', 'torneo_e2e'))).rejects.toThrow(
      /must point at/,
    );
    await expect(ageSession('x', 11, url('127.0.0.1', 'torneo'))).rejects.toThrow(/torneo_e2e/);
    await expect(
      ageSession('x', 11, `${url('127.0.0.1', 'torneo_e2e')}?host=db.example.com`),
    ).rejects.toThrow(/no query string or fragment/);
  });
});

describe('reset connects to the maintenance database', () => {
  it('postgres, never torneo_e2e', () => {
    const target = assertE2eTarget(url('postgres', 'torneo_e2e'));
    const maintenance = new URL(maintenanceUrl(target));
    expect(maintenance.pathname).toBe('/postgres');
    expect(maintenance.hostname).toBe('postgres');
    expect(maintenance.password).toBe(PASSWORD);
  });
});

describe('a9 --apply', () => {
  it('needs E2E_APPLY_A9=1', () => {
    expect(message(() => assertA9Apply(url('postgres', 'torneo_e2e'), {}))).toMatch(
      /E2E_APPLY_A9=1/,
    );
    expect(
      message(() => assertA9Apply(url('postgres', 'torneo_e2e'), { E2E_APPLY_A9: 'true' })),
    ).toMatch(/E2E_APPLY_A9=1/);
  });

  it.each(['127.0.0.1', 'localhost', '[::1]'])('never on loopback (%s)', (host) => {
    expect(message(() => assertA9Apply(url(host, 'torneo_e2e'), { E2E_APPLY_A9: '1' }))).toMatch(
      /loopback/,
    );
  });

  it('runs on the CI service', () => {
    expect(assertA9Apply(url('postgres', 'torneo_e2e'), { E2E_APPLY_A9: '1' }).hostname).toBe(
      'postgres',
    );
  });

  it('still checks the target first', () => {
    expect(message(() => assertA9Apply(url('postgres', 'torneo'), { E2E_APPLY_A9: '1' }))).toMatch(
      /torneo_e2e/,
    );
  });
});

describe('the A9 parameters come from infra/compose.dev.yml', () => {
  it('the ten parameters of ADR-0012 A9', () => {
    const params = a9Parameters();
    expect(params.map(([name]) => name)).toEqual([
      'log_error_verbosity',
      'log_min_error_statement',
      'log_statement',
      'log_min_duration_statement',
      'log_min_duration_sample',
      'log_connections',
      'log_disconnections',
      'log_transaction_sample_rate',
      'log_statement_sample_rate',
      'log_lock_waits',
    ]);
    const compose = readFileSync(
      new URL('../../../infra/compose.dev.yml', import.meta.url),
      'utf8',
    );
    for (const [name, value] of params) expect(compose).toContain(`${name}=${value}`);
  });

  it('a value changed in the file changes what is checked', () => {
    expect(a9Parameters('      - log_statement=all\n      - shared_buffers=1GB\n')).toEqual([
      ['log_statement', 'all'],
    ]);
  });
});
