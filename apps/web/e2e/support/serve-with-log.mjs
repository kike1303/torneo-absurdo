#!/usr/bin/env node
// Runs a process of the e2e and copies every stdout/stderr chunk to test-results/logs/<name>.log
// with appendFileSync, so the file never lags behind the process (plan-a decision 11). Writes
// its pid to test-results/logs/<name>.pid so the global teardown can stop it and wait for the
// exit.
//
//   serve-with-log.mjs <name>                    `node build` (the web)
//   serve-with-log.mjs <name> <command> [args]   any other process (008: the server)
//
// SOCKET_PATH and LISTEN_FDS are removed from the child's environment: adapter-node prefers
// them over HOST, and every process of the e2e listens on 127.0.0.1 (008 plan-d decision 10).
import { spawn } from 'node:child_process';
import { appendFileSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const [name, command, ...args] = process.argv.slice(2);
if (!name || !/^[a-z0-9-]+$/.test(name)) {
  process.stderr.write('usage: serve-with-log.mjs <name> [<command> [args...]]\n');
  process.exit(2);
}
const WEB = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const dir = path.join(WEB, 'test-results', 'logs');
mkdirSync(dir, { recursive: true });
const logFile = path.join(dir, `${name}.log`);
const pidFile = path.join(dir, `${name}.pid`);
writeFileSync(logFile, '');
writeFileSync(pidFile, String(process.pid));

const env = { ...process.env };
delete env.SOCKET_PATH;
delete env.LISTEN_FDS;

const child = command
  ? spawn(command, args, { cwd: WEB, env, stdio: ['ignore', 'pipe', 'pipe'] })
  : spawn(process.execPath, [path.join(WEB, 'build')], {
      cwd: WEB,
      env,
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
child.on('error', (error) => {
  appendFileSync(logFile, `serve-with-log: ${name} did not start (${error.name})\n`);
  rmSync(pidFile, { force: true });
  process.exit(1);
});
child.on('exit', (code, signal) => {
  rmSync(pidFile, { force: true });
  process.exit(stopping ? 0 : (code ?? (signal ? 1 : 0)));
});
