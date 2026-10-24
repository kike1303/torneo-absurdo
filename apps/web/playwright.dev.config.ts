import { defineConfig, devices } from '@playwright/test';

// Optional e2e against `just dev` (Vite on :5173 with the proxy to the server). Not part of
// `just e2e` nor CI. Run with: pnpm --filter web exec playwright test -c playwright.dev.config.ts
export default defineConfig({
  testDir: './e2e',
  testMatch: ['dev-status.spec.ts'],
  outputDir: './test-results/dev-artifacts',
  fullyParallel: false,
  workers: 1,
  retries: 0,
  reporter: [['list']],
  use: {
    baseURL: 'http://localhost:5173',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [
    { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
    { name: 'webkit', use: { ...devices['Desktop Safari'] } },
  ],
});
