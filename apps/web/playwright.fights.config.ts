import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
import { FIGHT_SCRIPT } from './e2e/support/fight-script.mjs';
import { CHROMIUM_FLAGS, stackServers } from './e2e/support/stack.js';
import { EGRESS_PROXY, PUBLIC_ORIGIN } from './e2e/support/topology.js';

// The real fights (010 preocupaciones 9 and 10, plan-d decisions 16 and 17). A run apart, after the
// general one: it is the only way for Chromium and WebKit, on desktop and at 375 px, to watch the
// same fights in parallel, without raising the workers of everything else. The grid is the smallest
// RF11 admits, and a fight takes what it takes: without retries, and with a limit for the whole run.
const GLOBAL_TIMEOUT_MS = 8 * 60_000;

export default defineConfig({
  testDir: './e2e',
  testMatch: ['fights.spec.ts'],
  outputDir: './test-results/fights-artifacts',
  fullyParallel: false,
  workers: 4,
  forbidOnly: !!process.env.CI,
  retries: 0,
  globalTimeout: GLOBAL_TIMEOUT_MS,
  reporter: [['list']],
  globalTeardown: './e2e/support/global-teardown.ts',
  use: {
    baseURL: PUBLIC_ORIGIN,
    // On this canvas a trace multiplies the time by four (TA-32), and here time is what is measured.
    trace: 'off',
    screenshot: 'only-on-failure',
    proxy: EGRESS_PROXY,
  },
  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'], launchOptions: CHROMIUM_FLAGS },
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] },
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
    },
    {
      name: 'webkit-mobile',
      use: { ...devices['iPhone 13 mini'] },
    },
  ],
  webServer: stackServers({
    FIGHT_SCRIPT_PATH: path.resolve(FIGHT_SCRIPT),
    FIGHT_CADENCE_SECS: '113',
    FIGHT_BETTING_WINDOW_SECS: '5',
    FIGHT_PRE_LIVE_SECS: '3',
  }),
});
