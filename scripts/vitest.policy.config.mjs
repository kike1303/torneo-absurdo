import { defineConfig } from 'vitest/config';

// The tests of check-workflows.mjs only, for the CI job `policy` (RF-C10, RF-C12).
export default defineConfig({
  test: {
    root: new URL('..', import.meta.url).pathname,
    include: ['scripts/__tests__/check-workflows.test.mjs'],
    environment: 'node',
  },
});
