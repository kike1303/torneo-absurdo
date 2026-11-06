import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
import { CHROMIUM_FLAGS, stackServers } from './e2e/support/stack.js';
import { EGRESS_PROXY, PUBLIC_ORIGIN } from './e2e/support/topology.js';

// e2e against `node build` and the server (RF-C10, ADR-0009 D5 amended, 008 RF-D7 and plan-d
// decision 10). Everything listens on 127.0.0.1:
//
//   browser ──▶ edge 4173 ──/api, /ws──▶ server 4180 (dev-auth) ──▶ Postgres torneo_e2e
//                  └──── the rest ────▶ web with catalog 4175 ──INTERNAL_API_URL──▶ server
//   web without catalog 4174 (no edge; its `/` asks the server for the fights, 010 RF53)
//   anything that is not loopback ──▶ egress proxy 4190 (records the host and cuts it)
//
// `just _e2e-run` generates the secrets once per run and passes them in the environment:
// e2e/support/stack.ts only reads them, because Playwright evaluates this again in every worker.
const AUTH = 'auth/*.spec.ts';
/** A roster that does not load: `/` has no fights, always, whatever the clock says (010 plan-d). */
const NO_ROSTER = path.join(import.meta.dirname, 'e2e', 'support', 'no-roster');

export default defineConfig({
  testDir: './e2e',
  // Runs against `just dev` with playwright.dev.config.ts, and with playwright.fights.config.ts.
  testIgnore: ['dev-status.spec.ts', 'fights.spec.ts'],
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
      testIgnore: ['dev-status.spec.ts', 'arena-webkit.spec.ts', 'fights.spec.ts'],
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] },
      // WebKit runs the theme and CSP tests (plan-a, file table), the draw to the end (005) and
      // the network guard (008).
      testMatch: [
        'theme.spec.ts',
        'csp.spec.ts',
        'arena-webkit.spec.ts',
        'egress.spec.ts',
        'home.spec.ts',
      ],
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
  webServer: stackServers({ CONTENT_DIR: NO_ROSTER }),
});
