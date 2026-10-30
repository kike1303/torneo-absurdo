#!/usr/bin/env node
// 008 plan-d decision 10, condition 1 of devops (round 2): `just _e2e-run` calls this before
// Playwright, so a leftover of an earlier local run never declares hosts or canaries for this one
// (that would weaken the guard) and an old egress.log never stands in for a proxy that did not
// start. Node built-ins only (it also runs in the Playwright image of CI).
//
//   node apps/web/e2e/support/reset-run.mjs [logs dir]   (default: apps/web/test-results/logs)
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

/** Emptied: what the specs declare (hosts, canaries) and the log of the ban CLI. */
export const RUN_FILES = ['expected-egress.txt', 'dynamic-canaries.txt', 'admin-cli.log'];
/** Removed: the egress proxy creates it empty when it starts, and the teardown needs it. */
export const STARTED_BY_THE_PROXY = 'egress.log';

/** @param {string} dir */
export function resetRunFiles(dir) {
  mkdirSync(dir, { recursive: true });
  rmSync(path.join(dir, STARTED_BY_THE_PROXY), { force: true });
  for (const name of RUN_FILES) writeFileSync(path.join(dir, name), '');
}

if (process.argv[1] && fileURLToPath(import.meta.url) === path.resolve(process.argv[1])) {
  const fallback = path.resolve(
    path.dirname(fileURLToPath(import.meta.url)),
    '../../test-results/logs',
  );
  resetRunFiles(process.argv[2] ? path.resolve(process.argv[2]) : fallback);
}
