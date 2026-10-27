import { defineConfig, devices } from '@playwright/test';

// e2e against `node build` (RF-C10, ADR-0009 D5 amended): two instances, with and without the
// catalog, each with PUBLIC_ORIGIN pointing at itself. Logs are captured by serve-with-log.mjs.
const CATALOG_PORT = 4173;
const NO_CATALOG_PORT = 4174;

const baseEnv = {
  NODE_ENV: 'production',
  // getClientAddress() reads the canary X-Forwarded-For (RF-A19 e2e).
  ADDRESS_HEADER: 'X-Forwarded-For',
  XFF_DEPTH: '1',
};

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
    baseURL: `http://localhost:${CATALOG_PORT}`,
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    {
      name: 'chromium',
      use: {
        ...devices['Desktop Chrome'],
        // WebGL by software for /dev/arena, also in the CI container (002 plan-a2).
        launchOptions: { args: ['--use-angle=swiftshader', '--enable-unsafe-swiftshader'] },
      },
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] },
      // WebKit runs the theme and CSP tests (plan-a, file table).
      testMatch: ['theme.spec.ts', 'csp.spec.ts'],
    },
  ],
  webServer: [
    {
      command: 'node e2e/support/serve-with-log.mjs catalog',
      url: `http://localhost:${CATALOG_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 30_000,
      env: {
        ...baseEnv,
        PORT: String(CATALOG_PORT),
        PUBLIC_ORIGIN: `http://localhost:${CATALOG_PORT}`,
        ENABLE_DEV_CATALOG: '1',
      },
    },
    {
      command: 'node e2e/support/serve-with-log.mjs no-catalog',
      url: `http://localhost:${NO_CATALOG_PORT}/healthz`,
      reuseExistingServer: false,
      timeout: 30_000,
      env: {
        ...baseEnv,
        PORT: String(NO_CATALOG_PORT),
        PUBLIC_ORIGIN: `http://localhost:${NO_CATALOG_PORT}`,
      },
    },
  ],
});
