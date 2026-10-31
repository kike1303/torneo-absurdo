#!/usr/bin/env node
// The database of the e2e (008 plan-d decision 10): `torneo_e2e` from scratch, the log
// parameters of ADR-0012 A9 and two helpers for the specs. Every operation goes through
// `assertE2eTarget` first (change 6 of devops, 5 of architect), whose messages name the rule and
// never the URL (it carries the password).
//
//   node e2e/support/db.mjs reset        drop and create torneo_e2e (from the `postgres` db)
//   node e2e/support/db.mjs a9 --apply   ALTER SYSTEM the A9 parameters (CI only, never loopback)
//   node e2e/support/db.mjs a9 --check   fail if a parameter differs (always)
//
// The URL comes from E2E_DATABASE_URL.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import postgres from 'postgres';

export const E2E_DATABASE = 'torneo_e2e';
const ALLOWED_HOSTS = ['127.0.0.1', 'localhost', '[::1]', 'postgres'];
const LOOPBACK_HOSTS = ['127.0.0.1', 'localhost', '[::1]'];
const MAINTENANCE_DATABASE = 'postgres';
const ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../../..');
const COMPOSE_DEV = path.join(ROOT, 'infra', 'compose.dev.yml');

/**
 * The parsed URL if it points at the e2e database on an allowed host; otherwise it throws with a
 * message that names the rule only.
 * @param {string | undefined} url
 * @returns {URL}
 */
export function assertE2eTarget(url) {
  if (!url) throw new Error('E2E_DATABASE_URL is required');
  if (!URL.canParse(url)) throw new Error('E2E_DATABASE_URL is not a URL');
  const parsed = new URL(url);
  // No query or fragment at all, an empty `?` or `#` included: sqlx takes `host`, `hostaddr` and
  // `dbname` from the query and postgres.js takes `database`, so any parameter could send the
  // server, the seeding or the ban CLI to another database behind a URL that passes the rest.
  if (parsed.search || parsed.hash || url.includes('?') || url.includes('#')) {
    throw new Error('E2E_DATABASE_URL must carry no query string or fragment');
  }
  if (parsed.protocol !== 'postgres:' && parsed.protocol !== 'postgresql:') {
    throw new Error('E2E_DATABASE_URL must be a postgres:// URL');
  }
  if (decodeURIComponent(parsed.pathname.slice(1)) !== E2E_DATABASE) {
    throw new Error(`E2E_DATABASE_URL must name exactly the database ${E2E_DATABASE}`);
  }
  if (!ALLOWED_HOSTS.includes(parsed.hostname)) {
    throw new Error(`E2E_DATABASE_URL must point at one of ${ALLOWED_HOSTS.join(', ')}`);
  }
  return parsed;
}

/** The same server and credentials, on the maintenance database `postgres`. @param {URL} url */
export function maintenanceUrl(url) {
  const copy = new URL(url);
  copy.pathname = `/${MAINTENANCE_DATABASE}`;
  return copy.toString();
}

/**
 * The A9 parameters of `infra/compose.dev.yml` (`-c log_*=value`), read from the file and never
 * copied, so a change there reaches the e2e.
 * @param {string} [text]
 * @returns {[string, string][]}
 */
export function a9Parameters(text = readFileSync(COMPOSE_DEV, 'utf8')) {
  /** @type {[string, string][]} */
  const out = [];
  for (const match of text.matchAll(/^\s*-\s*(log_[a-z_]+)=(\S+)\s*$/gm)) {
    out.push([match[1] ?? '', match[2] ?? '']);
  }
  return out;
}

/** @param {string} url */
function connect(url) {
  return postgres(url, { max: 1, onnotice: () => undefined, connect_timeout: 5 });
}

/**
 * Drops and creates torneo_e2e, connected to `postgres` (never to the database it drops). Retries
 * the connection for up to 60 s: the CI service container may still be starting.
 * @param {string | undefined} url
 */
export async function reset(url) {
  const target = assertE2eTarget(url);
  const deadline = Date.now() + 60_000;
  for (;;) {
    const sql = connect(maintenanceUrl(target));
    try {
      await sql`SELECT 1`;
      await sql.unsafe(`DROP DATABASE IF EXISTS ${E2E_DATABASE} WITH (FORCE)`);
      await sql.unsafe(`CREATE DATABASE ${E2E_DATABASE}`);
      return;
    } catch (error) {
      const code = /** @type {{ code?: string }} */ (error).code;
      // Only connection errors are retried; an SQL error is final. The driver's error is not
      // attached as `cause`: it may carry connection details, and only the code is printed.
      if (
        code &&
        !/^(ECONNREFUSED|ECONNRESET|ENOTFOUND|EAI_AGAIN|CONNECT_TIMEOUT|57P03)$/.test(code)
      )
        // eslint-disable-next-line preserve-caught-error -- see above
        throw new Error(`reset failed (${code})`);
      // eslint-disable-next-line preserve-caught-error -- see above
      if (Date.now() > deadline) throw new Error('reset: Postgres did not answer within 60 s');
      await new Promise((resolve) => setTimeout(resolve, 1000));
    } finally {
      await sql.end({ timeout: 1 });
    }
  }
}

/**
 * `a9 --apply` only with E2E_APPLY_A9=1 and never on loopback: ALTER SYSTEM writes
 * postgresql.auto.conf into the volume, and must never touch the Postgres of `just dev`.
 * @param {string | undefined} url
 * @param {Record<string, string | undefined>} env
 */
export function assertA9Apply(url, env) {
  const target = assertE2eTarget(url);
  if (env.E2E_APPLY_A9 !== '1') throw new Error('a9 --apply needs E2E_APPLY_A9=1');
  if (LOOPBACK_HOSTS.includes(target.hostname)) {
    throw new Error('a9 --apply never runs on a loopback host (the Postgres of just dev)');
  }
  return target;
}

/**
 * @param {string | undefined} url
 * @param {Record<string, string | undefined>} env
 */
export async function applyA9(url, env) {
  const target = assertA9Apply(url, env);
  const sql = connect(maintenanceUrl(target));
  try {
    for (const [name, value] of a9Parameters()) {
      // Names come from a fixed pattern of our own file; the value is passed as a literal.
      if (!/^log_[a-z_]+$/.test(name)) throw new Error('a9: unexpected parameter name');
      await sql.unsafe(`ALTER SYSTEM SET ${name} = ${quote(value)}`);
    }
    await sql`SELECT pg_reload_conf()`;
  } finally {
    await sql.end({ timeout: 1 });
  }
}

/** A single-quoted SQL literal. @param {string} value */
function quote(value) {
  return `'${value.replaceAll("'", "''")}'`;
}

/**
 * The parameters whose value differs from compose.dev.yml, as `name` only (never a value).
 * @param {string | undefined} url
 */
export async function checkA9(url) {
  const target = assertE2eTarget(url);
  const sql = connect(target.toString());
  try {
    const wrong = [];
    for (const [name, expected] of a9Parameters()) {
      const [row] = await sql`SELECT current_setting(${name}) AS value`;
      if (String(row?.value) !== expected) wrong.push(name);
    }
    return wrong;
  } finally {
    await sql.end({ timeout: 1 });
  }
}

/**
 * The `users.id` of a `dev` identity (for the ban CLI).
 * @param {string} identity
 * @param {string | undefined} [url]
 */
export async function userId(identity, url = process.env.E2E_DATABASE_URL) {
  const target = assertE2eTarget(url);
  const sql = connect(target.toString());
  try {
    const [row] = await sql`
      SELECT user_id::text AS id FROM oauth_accounts
      WHERE provider = 'dev' AND provider_user_id = ${identity}`;
    if (!row) throw new Error('userId: no user for that identity');
    return String(row.id);
  } finally {
    await sql.end({ timeout: 1 });
  }
}

/**
 * Links an account of `provider` to the user of a `dev` identity, without the provider (the e2e
 * has no network): for the unlink tests of account.spec.ts. The `id` comes from the sequence, so
 * the session in use stops being current until it reauthenticates (009 RF10).
 * @param {string} identity
 * @param {'google' | 'discord'} provider
 * @param {string | undefined} [url]
 */
export async function seedOauthAccount(identity, provider, url = process.env.E2E_DATABASE_URL) {
  const target = assertE2eTarget(url);
  const sql = connect(target.toString());
  try {
    await sql`
      INSERT INTO oauth_accounts (provider, provider_user_id, user_id)
      SELECT ${provider}, ${`e2e-seed-${identity}`}, user_id FROM oauth_accounts
      WHERE provider = 'dev' AND provider_user_id = ${identity}`;
  } finally {
    await sql.end({ timeout: 1 });
  }
}

/**
 * Moves the authentication of every session of a `dev` identity back in time (009): the e2e has
 * no clock to wait for. Only `authenticated_at`, the column that the 10 minutes read.
 * @param {string} identity
 * @param {number} minutes
 * @param {string | undefined} [url]
 */
export async function ageSession(identity, minutes, url = process.env.E2E_DATABASE_URL) {
  const target = assertE2eTarget(url);
  const sql = connect(target.toString());
  try {
    await sql`
      UPDATE sessions SET authenticated_at = authenticated_at - make_interval(mins => ${minutes}::int)
      WHERE user_id = (
        SELECT user_id FROM oauth_accounts WHERE provider = 'dev' AND provider_user_id = ${identity}
      )`;
  } finally {
    await sql.end({ timeout: 1 });
  }
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  const [command, flag] = process.argv.slice(2);
  const url = process.env.E2E_DATABASE_URL;
  try {
    if (command === 'reset') {
      await reset(url);
      console.log(`db: ${E2E_DATABASE} created`);
    } else if (command === 'a9' && flag === '--apply') {
      await applyA9(url, process.env);
      console.log('db: A9 parameters applied');
    } else if (command === 'a9' && flag === '--check') {
      const wrong = await checkA9(url);
      if (wrong.length > 0)
        throw new Error(`a9: parameters differ from compose.dev.yml: ${wrong.join(', ')}`);
      console.log(`db: the ${a9Parameters().length} A9 parameters match compose.dev.yml`);
    } else {
      throw new Error('usage: db.mjs reset | a9 --apply | a9 --check');
    }
  } catch (error) {
    console.error(`db: ${error instanceof Error ? error.message : 'failed'}`);
    process.exit(1);
  }
}
