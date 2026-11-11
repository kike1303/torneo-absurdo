import { defineConfig } from 'vitest/config';

// The tests of the CI job `policy`: check-workflows.mjs (RF-C10, RF-C12) and check-migrations.mjs
// (TA-46).
export default defineConfig({
  test: {
    root: new URL('..', import.meta.url).pathname,
    include: [
      'scripts/__tests__/check-workflows.test.mjs',
      'scripts/__tests__/check-migrations.test.mjs',
    ],
    environment: 'node',
    setupFiles: ['scripts/vitest.setup.mjs'],
  },
});
