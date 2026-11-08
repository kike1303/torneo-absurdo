import path from 'node:path';
import { defineConfig, devices } from '@playwright/test';
import { BETS_SCRIPT } from './e2e/support/fight-script.mjs';
import { CHROMIUM_FLAGS, stackServers } from './e2e/support/stack.js';
import { EGRESS_PROXY, PUBLIC_ORIGIN } from './e2e/support/topology.js';

// The bets on real fights (012 plan-c C8): three fights of the script, a KO with both sides, a draw
// and a KO with one side only, watched by the four projects at once with their own accounts. The
// window is 40 s, enough for each project to bet, and the cadence the smallest RF11 of 010 admits
// with it. Like the fights run, it is local only (TA-56): the page mounts the 3D stage.
const GLOBAL_TIMEOUT_MS = 13 * 60_000;

export default defineConfig({
  testDir: './e2e',
  testMatch: ['bets.spec.ts'],
  outputDir: './test-results/bets-artifacts',
  fullyParallel: false,
  workers: 4,
  forbidOnly: !!process.env.CI,
  retries: 0,
  globalTimeout: GLOBAL_TIMEOUT_MS,
  reporter: [['list']],
  globalTeardown: './e2e/support/global-teardown.ts',
  use: {
    baseURL: PUBLIC_ORIGIN,
    trace: 'off',
    screenshot: 'only-on-failure',
    proxy: EGRESS_PROXY,
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'], launchOptions: CHROMIUM_FLAGS } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
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
    { name: 'webkit-mobile', use: { ...devices['iPhone 13 mini'] } },
  ],
  webServer: stackServers({
    FIGHT_SCRIPT_PATH: path.resolve(BETS_SCRIPT),
    CONTENT_DIR: path.resolve(import.meta.dirname, '../../content/characters'),
    FIGHT_CADENCE_SECS: '148',
    FIGHT_BETTING_WINDOW_SECS: '40',
    FIGHT_PRE_LIVE_SECS: '3',
  }),
});
