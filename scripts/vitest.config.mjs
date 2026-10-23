import { defineConfig } from 'vitest/config';

// Tests of the repo scripts (ADR-0009 test 1 and the i18n, prerender and Kit config fixtures).
export default defineConfig({
  test: {
    root: new URL('..', import.meta.url).pathname,
    include: ['scripts/__tests__/**/*.test.mjs'],
    environment: 'node',
  },
});
