import path from 'node:path';
import {
  CATALOG_PORT,
  EDGE_PORT,
  EGRESS_PORT,
  LOOPBACK,
  NO_CATALOG_PORT,
  PUBLIC_ORIGIN,
  SERVER_PORT,
} from './topology.js';
import { assertE2eTarget } from './db.mjs';

// The processes of the e2e (008 plan-d decision 10), shared by playwright.config.ts and
// playwright.fights.config.ts (010 plan-d decision 16): only the variables of the server change.
// `just _e2e-run` generates the secrets once per run and passes them in the environment; this
// file only reads them, because Playwright evaluates it again in every worker.
const INTERNAL_API_URL = `http://${LOOPBACK}:${SERVER_PORT}`;
const LOGS = path.join(import.meta.dirname, '..', '..', 'test-results', 'logs');

function required(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`${name} is required: run the e2e with \`just e2e\` (just _e2e-run sets it)`);
  }
  return value;
}

const secret = required('E2E_INTERNAL_API_SECRET');
const ipHashKey = required('E2E_IP_HASH_KEY');
// The same guard as the seeding and the ban CLI: the dev-auth server gets this URL too.
const databaseUrl = required('E2E_DATABASE_URL');
assertE2eTarget(databaseUrl);
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
export const CHROMIUM_FLAGS = { args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] };

/** Every process, with these variables added to the server's. */
export function stackServers(serverEnv: Record<string, string>) {
  return [
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
        ...serverEnv,
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
  ];
}
