#!/usr/bin/env node
// Runs `node build` and copies every stdout/stderr chunk to test-results/logs/<name>.log with
// appendFileSync, so the file never lags behind the process (plan-a decision 11). Writes its pid
// to test-results/logs/<name>.pid so the global teardown can stop it and wait for the exit.
import { spawn } from 'node:child_process';
import { appendFileSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const name = process.argv[2];
if (!name) {
  process.stderr.write('usage: serve-with-log.mjs <name>\n');
  process.exit(2);
}
const WEB = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const dir = path.join(WEB, 'test-results', 'logs');
mkdirSync(dir, { recursive: true });
const logFile = path.join(dir, `${name}.log`);
const pidFile = path.join(dir, `${name}.pid`);
writeFileSync(logFile, '');
writeFileSync(pidFile, String(process.pid));

const child = spawn(process.execPath, [path.join(WEB, 'build')], {
  cwd: WEB,
  env: process.env,
  stdio: ['ignore', 'pipe', 'pipe'],
});
child.stdout.on('data', (chunk) => appendFileSync(logFile, chunk));
child.stderr.on('data', (chunk) => appendFileSync(logFile, chunk));

let stopping = false;
const stop = () => {
  if (stopping) return;
  stopping = true;
  child.kill('SIGTERM');
};
process.on('SIGTERM', stop);
process.on('SIGINT', stop);
child.on('exit', (code, signal) => {
  rmSync(pidFile, { force: true });
  process.exit(stopping ? 0 : (code ?? (signal ? 1 : 0)));
});
