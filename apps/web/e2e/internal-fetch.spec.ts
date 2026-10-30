import { type ChildProcess, spawn } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { type IncomingHttpHeaders, type Server, createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import path from 'node:path';
import { expect, test } from '@playwright/test';
import { WEB_DIR, addDynamicCanary } from './support/logs.js';
import { raw } from './support/raw-http.js';
import { INTERNAL_FETCH_WEB_PORT, LOOPBACK } from './support/topology.js';

// QA of phase D (008 RF-D2, plan-d decision 21): Kit's real `event.fetch` adds every browser
// cookie when the target's hostname matches `event.url.hostname`, which comes from `Host`. This
// spec runs `node build` against a recording API on 127.0.0.1 and sends `Host: 127.0.0.1:…`
// with a canary cookie: only the session cookie of RF-D2 may ever reach the API. The unit
// tests of handle-fetch.ts stub `fetch`, so they cannot see what Kit adds after the hook.

const WEB = INTERNAL_FETCH_WEB_PORT;
const TOKEN = randomBytes(24).toString('base64url');
const CANARY = `canary-cookie-${randomBytes(4).toString('hex')}`;

interface Seen {
  url: string;
  headers: IncomingHttpHeaders;
}

let api: Server;
let apiPort = 0;
let web: ChildProcess;
let output = '';
const seen: Seen[] = [];

test.beforeAll(async () => {
  addDynamicCanary(TOKEN);
  addDynamicCanary(CANARY);
  api = createServer((req, res) => {
    seen.push({ url: req.url ?? '', headers: req.headers });
    // 401 to everything: the session is dead, the page renders without the server.
    res.writeHead(401, { 'content-type': 'application/json' }).end('{"code":"auth.unauthorized"}');
  });
  await new Promise<void>((resolve) => api.listen(0, LOOPBACK, resolve));
  apiPort = (api.address() as AddressInfo).port;

  const secret = process.env.E2E_INTERNAL_API_SECRET;
  if (!secret) throw new Error('E2E_INTERNAL_API_SECRET is required: run `just e2e`');
  const env: NodeJS.ProcessEnv = {
    PATH: process.env.PATH,
    NODE_ENV: 'production',
    HOST: LOOPBACK,
    PORT: String(WEB),
    // Loopback http, so `ta_dev_session` is the policy cookie (RF-D2).
    PUBLIC_ORIGIN: `http://localhost:${WEB}`,
    INTERNAL_API_URL: `http://${LOOPBACK}:${apiPort}`,
    INTERNAL_API_SECRET: secret,
    PROTOCOL_HEADER: 'x-forwarded-proto',
  };
  web = spawn(process.execPath, [path.join(WEB_DIR, 'build')], {
    cwd: WEB_DIR,
    env,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  web.stdout?.on('data', (chunk: Buffer) => (output += chunk.toString('utf8')));
  web.stderr?.on('data', (chunk: Buffer) => (output += chunk.toString('utf8')));
  await expect
    .poll(async () => (await raw(WEB, '/healthz').catch(() => ({ status: 0 }))).status, {
      timeout: 15_000,
    })
    .toBe(200);
});

test.afterAll(async () => {
  if (web && web.exitCode === null) {
    const exited = new Promise((resolve) => web.once('exit', resolve));
    web.kill('SIGTERM');
    await exited;
  }
  await new Promise((resolve) => api?.close(resolve));
  // The process of this spec has no log file of its own: its output is checked here.
  expect(output).not.toContain(TOKEN);
  expect(output).not.toContain(CANARY);
});

const cookieHeader = `ta_dev_session=${TOKEN}; other=${CANARY}; theme=dark`;

for (const host of [
  `localhost:${WEB}`,
  // The hostname of INTERNAL_API_URL, another port: Kit's passthrough would add every cookie.
  `${LOOPBACK}:${WEB}`,
  `${LOOPBACK}`,
]) {
  test(`Host ${host}: the API gets only the session cookie of RF-D2`, async () => {
    const before = seen.length;
    const res = await raw(WEB, '/es/login', {
      headers: { host, cookie: cookieHeader, 'x-forwarded-proto': 'http' },
    });
    expect(res.status).toBe(200);
    const calls = seen.slice(before);
    // The session read and the page's providers read, both towards the recording API.
    expect(calls.map((c) => c.url)).toContain('/api/me');
    for (const call of calls) {
      expect(call.headers.cookie, call.url).toBe(`ta_dev_session=${TOKEN}`);
    }
    expect(res.body).not.toContain(CANARY);
  });
}

test('Host equal to INTERNAL_API_URL: no cookie but the policy one leaves, nothing loops', async () => {
  const before = seen.length;
  const res = await raw(WEB, '/es/login', {
    headers: { host: `${LOOPBACK}:${apiPort}`, cookie: cookieHeader, 'x-forwarded-proto': 'http' },
  });
  expect(res.status).toBeLessThan(500);
  for (const call of seen.slice(before)) {
    expect(call.headers.cookie, call.url).toBe(`ta_dev_session=${TOKEN}`);
  }
  expect(res.body).not.toContain(CANARY);
  expect(res.body).not.toContain(process.env.E2E_INTERNAL_API_SECRET ?? '\u0000');
});
