import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
import {
  CATALOG_PORT,
  EDGE_PORT,
  EGRESS_PORT,
  EGRESS_PROXY,
  LOOPBACK,
  NO_CATALOG_PORT,
  PUBLIC_ORIGIN,
  SERVER_PORT,
} from './e2e/support/topology.js';

// e2e against `node build` and the server (RF-C10, ADR-0009 D5 amended, 008 RF-D7 and plan-d
// decision 10). Everything listens on 127.0.0.1:
//
//   browser ──▶ edge 4173 ──/api, /ws──▶ server 4180 (dev-auth) ──▶ Postgres torneo_e2e
//                  └──── the rest ────▶ web with catalog 4175 ──INTERNAL_API_URL──▶ server
//   web without catalog 4174 (no edge; it never calls the server without a session cookie)
//   anything that is not loopback ──▶ egress proxy 4190 (records the host and cuts it)
//
// `just _e2e-run` generates the secrets once per run and passes them in the environment; this
// file only reads them, because Playwright evaluates it again in every worker.
const INTERNAL_API_URL = `http://${LOOPBACK}:${SERVER_PORT}`;
const LOGS = path.join(import.meta.dirname, 'test-results', 'logs');

function required(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required: run the e2e with \`just e2e\` (just _e2e-run sets it)`);
  }
  return value;
}

const secret = required('E2E_INTERNAL_API_SECRET');
const ipHashKey = required('E2E_IP_HASH_KEY');
const databaseUrl = required('E2E_DATABASE_URL');
const serverBin = required('E2E_SERVER_BIN');
if (!path.isAbsolute(serverBin)) {
  throw new Error('E2E_SERVER_BIN must be an absolute path (Playwright runs in apps/web)');
}

const webEnv = {
  NODE_ENV: 'production',
  HOST: LOOPBACK,
  INTERNAL_API_URL,
  INTERNAL_API_SECRET: secret,
  // getClientAddress() reads the X-Forwarded-For of the edge (and the canary of logs.spec.ts).
  ADDRESS_HEADER: 'X-Forwarded-For',
  XFF_DEPTH: '1',
  // The edge says `http` (like Traefik says `https`): without it adapter-node assumes https and
  // Kit refuses every form action of the http e2e as cross-site. Production needs nothing:
  // there the default (https) is right.
  PROTOCOL_HEADER: 'x-forwarded-proto',
};

/** WebGL by software for /dev/arena, also in the CI container (002 plan-a2). */
const CHROMIUM_FLAGS = { args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] };
const AUTH = 'auth/*.spec.ts';

export default defineConfig({
  testDir: './e2e',
  // Runs against `just dev` with playwright.dev.config.ts, never here.
  testIgnore: ['dev-status.spec.ts'],
  outputDir: './test-results/artifacts',
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  reporter: [['list']],
  globalTeardown: './e2e/support/global-teardown.ts',
  use: {
    baseURL: PUBLIC_ORIGIN,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
    // The network guard (change 1 of architect): what is not loopback goes to the egress proxy,
    // which records it and cuts it, the destination of a redirect included.
    proxy: EGRESS_PROXY,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], launchOptions: CHROMIUM_FLAGS },
      // Covered by arena.spec.ts in Chromium (005 plan decision 11).
      testIgnore: ['dev-status.spec.ts', 'arena-webkit.spec.ts'],
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] },
      // WebKit runs the theme and CSP tests (plan-a, file table), the draw to the end (005) and
      // the network guard (008).
      testMatch: ['theme.spec.ts', 'csp.spec.ts', 'arena-webkit.spec.ts', 'egress.spec.ts'],
    },
    {
      name: 'chromium-mobile',
      use: {
        ...devices['Desktop Chrome'],
        viewport: { width: 375, height: 812 },
        hasTouch: true,
        isMobile: true,
        deviceScaleFactor: 3,
        launchOptions: CHROMIUM_FLAGS,
      },
      testMatch: [AUTH],
    },
    {
      name: 'webkit-mobile',
      use: { ...devices['iPhone 13 mini'] },
      testMatch: [AUTH],
    },
  ],
  webServer: [
    {
      command: `node e2e/support/egress-proxy.mjs ${EGRESS_PORT} ${path.join(LOGS, 'egress.log')}`,
      url: `http://${LOOPBACK}:${EGRESS_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 10_000,
    },
    {
      command: `node e2e/support/serve-with-log.mjs server ${JSON.stringify(serverBin)}`,
      url: `${INTERNAL_API_URL}/api/health`,
      reuseExistingServer: false,
      timeout: 60_000,
      env: {
        DATABASE_URL: databaseUrl,
        PUBLIC_ORIGIN,
        BIND_ADDR: LOOPBACK,
        PORT: String(SERVER_PORT),
        TRUSTED_PROXIES: `${LOOPBACK}/32`,
        INTERNAL_API_SECRET: secret,
        IP_HASH_KEY: ipHashKey,
        DAILY_GRANT: '100',
        RUST_LOG: 'info',
        // Fillers, so the Google and Discord buttons exist (plan-d open question 1); the egress
        // proxy cuts the redirect to the provider.
        GOOGLE_CLIENT_ID: 'e2e-filler-google-id',
        GOOGLE_CLIENT_SECRET: 'e2e-filler-google-secret',
        DISCORD_CLIENT_ID: 'e2e-filler-discord-id',
        DISCORD_CLIENT_SECRET: 'e2e-filler-discord-secret',
      },
    },
    {
      command: 'node e2e/support/serve-with-log.mjs catalog',
      url: `http://${LOOPBACK}:${CATALOG_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 30_000,
      env: {
        ...webEnv,
        PORT: String(CATALOG_PORT),
        PUBLIC_ORIGIN,
        ENABLE_DEV_CATALOG: '1',
      },
    },
    {
      command: 'node e2e/support/serve-with-log.mjs no-catalog',
      url: `http://${LOOPBACK}:${NO_CATALOG_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 30_000,
      env: {
        ...webEnv,
        PORT: String(NO_CATALOG_PORT),
        PUBLIC_ORIGIN: `http://localhost:${NO_CATALOG_PORT}`,
      },
    },
    {
      command: `node e2e/support/edge.mjs ${EDGE_PORT} ${SERVER_PORT} ${CATALOG_PORT}`,
      url: `http://${LOOPBACK}:${EDGE_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 10_000,
    },
  ],
});
