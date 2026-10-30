import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
import { randomBytes } from 'node:crypto';
import { assertE2eTarget, userId } from '../support/db.mjs';
import { ADMIN_CLI_LOG, addDynamicCanary } from '../support/logs.js';
import { clientFor, expect, login, test, text } from './fixtures.js';

// 008 RF-D8 ("a user banned by the CLI sees their session close", read as answer 5 of architect:
// the next navigation has no session and the login gives auth.banned), plan-d decision 10.

/**
 * `server admin …` with only DATABASE_URL (the e2e one, after its guard) and RUST_LOG, never
 * inheriting the environment (change 9 of architect). Its output goes to admin-cli.log, which
 * the teardown checks for canaries.
 */
function admin(...args: string[]): void {
  const bin = process.env.E2E_SERVER_BIN ?? '';
  const url = assertE2eTarget(process.env.E2E_DATABASE_URL).toString();
  const run = spawnSync(bin, ['admin', ...args], {
    env: { DATABASE_URL: url, RUST_LOG: 'info' },
    encoding: 'utf8',
    timeout: 30_000,
  });
  appendFileSync(ADMIN_CLI_LOG, `${run.stdout ?? ''}${run.stderr ?? ''}`);
  expect(run.status, `admin ${args[0]}`).toBe(0);
}

test('a banned user loses the session and gets auth.banned; unban lets them back', async ({
  page,
  identity,
}, info) => {
  await login(page, identity, 'en', '/en/');
  const id = await userId(identity);
  addDynamicCanary(id);
  const reason = `e2e-ban-${randomBytes(6).toString('hex')}`;
  addDynamicCanary(reason);

  admin('ban', id, '--reason', reason);
  await page.goto('/en/');
  await expect(page.getByTestId('header-sign-in')).toBeVisible();

  await login(page, identity, 'en', '/en/');
  await expect(page).toHaveURL(/\/en\/login\?error=auth\.banned$/);
  await expect(page.getByTestId('login-error')).toHaveText(text('en', 'error_auth_banned'));

  admin('unban', id);
  // A third login within the minute would exhaust the AUTH_IP burst of this /64 (each login is
  // a start and a callback): the same user from another network, as a real one would retry.
  await page.context().setExtraHTTPHeaders({ 'x-e2e-client': clientFor(info, 'after-unban') });
  await login(page, identity, 'en', '/en/');
  await expect(page.getByTestId('header-user')).toBeVisible();
});
