import { defineConfig, devices } from '@playwright/test';
import { CHROMIUM_FLAGS, NO_ROSTER, stackServers } from './e2e/support/stack.js';
import { EGRESS_PROXY, PUBLIC_ORIGIN } from './e2e/support/topology.js';

// The seeded replays (011 "Estrategia de pruebas"): `just _e2e-fights-run` runs this after the real
// fights, on a new torneo_e2e. A KO, a decision, a draw, a voided, two cancelled and a scheduled one,
// with real data and no roster, so no other fight appears. One worker: the four projects share one
// IP and the 120 reads per minute of FIGHT_READ_IP (preocupacion 11). What is under `fights-artifacts`
// reaches the artifact of the job `e2e-fights` as it is.
//
// The flows run in the phones, the case of the spectator, and desktop only keeps what is of desktop:
// the CSP in both engines, axe, the hydration and the screenshots (preocupacion 12: the four projects
// took 5.5 min, against an objective of 3).
const DESKTOP = ['csp.spec.ts', 'a11y.spec.ts', 'hydration.spec.ts'];
export default defineConfig({
  testDir: './e2e',
  testMatch: ['replays.spec.ts', 'csp.spec.ts', 'a11y.spec.ts', 'hydration.spec.ts'],
  grep: /@replays/,
  outputDir: './test-results/fights-artifacts/replays',
  fullyParallel: false,
  workers: 1,
  forbidOnly: !!process.env.CI,
  retries: 0,
  timeout: 60_000,
  reporter: [['list']],
  globalSetup: './e2e/support/seed-replays.ts',
  globalTeardown: './e2e/support/global-teardown.ts',
  use: {
    baseURL: PUBLIC_ORIGIN,
    // On this canvas a trace multiplies the time by four (TA-32).
    trace: 'off',
    screenshot: 'only-on-failure',
    proxy: EGRESS_PROXY,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], launchOptions: CHROMIUM_FLAGS },
      testMatch: [...DESKTOP, 'replays.spec.ts'],
      // Playwright joins the tag of a `describe` right after its title.
      grep: /@replays.*what it looks like|(?:csp|a11y|hydration)\.spec.*@replays/,
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] },
      testMatch: ['csp.spec.ts'],
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
      testMatch: ['replays.spec.ts'],
    },
    {
      name: 'webkit-mobile',
      use: { ...devices['iPhone 13 Mini'] },
      testMatch: ['replays.spec.ts'],
    },
  ],
  webServer: stackServers({ CONTENT_DIR: NO_ROSTER }),
});
