import { defineConfig } from 'vitest/config';

// Tests of the repo scripts (ADR-0009 test 1 and the i18n, prerender and Kit config fixtures).
// check-workflows runs only in the CI job `policy` (scripts/vitest.policy.config.mjs, RF-C10).
export default defineConfig({
  test: {
    root: new URL('..', import.meta.url).pathname,
    include: ['scripts/__tests__/**/*.test.mjs'],
    exclude: ['scripts/__tests__/check-workflows.test.mjs'],
    environment: 'node',
  },
});
