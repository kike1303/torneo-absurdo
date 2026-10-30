import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import {
  ADMIN_CLI_LOG,
  CANARIES,
  DYNAMIC_CANARIES,
  EGRESS_LOG,
  EXPECTED_EGRESS,
  LOG_DIR,
  SERVER_LOG,
  fileLines,
  readLines,
  whitelistProblems,
  type Instance,
} from './logs.js';

const INSTANCES: Instance[] = ['catalog', 'no-catalog'];
/** Every process with a pid file of serve-with-log.mjs. */
const PROCESSES = [...INSTANCES, 'server'];

async function stop(name: string): Promise<void> {
  const pidFile = path.join(LOG_DIR, `${name}.pid`);
  if (!existsSync(pidFile)) return;
  const pid = Number(readFileSync(pidFile, 'utf8'));
  try {
    process.kill(pid, 'SIGTERM');
  } catch {
    return;
  }
  for (let i = 0; i < 100; i++) {
    try {
      process.kill(pid, 0);
    } catch {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`process ${name} (pid ${pid}) did not exit`);
}

/**
 * The canaries of this run (008 plan-d decision 10): the fixed ones, the two secrets that
 * `just _e2e-run` generated and what the specs declared (tokens, user ids, aliases, identities,
 * `?error=` values, ban reasons).
 */
function canaries(): string[] {
  const values: string[] = Object.values(CANARIES);
  for (const name of ['E2E_INTERNAL_API_SECRET', 'E2E_IP_HASH_KEY']) {
    const value = process.env[name];
    if (value) values.push(value);
  }
  values.push(...fileLines(DYNAMIC_CANARIES));
  return values;
}

/** Problems of a log that must have no canary (the server's lines are JSON objects). */
function canaryProblems(label: string, lines: string[], values: string[]): string[] {
  const problems: string[] = [];
  lines.forEach((line, index) => {
    for (const value of values) {
      // The value is never printed: only which kind of canary and where.
      if (line.includes(value)) problems.push(`${label}: line ${index + 1} contains a canary`);
    }
  });
  return problems;
}

/** The server writes one JSON object per line (ADR-0012 A1). */
function jsonProblems(label: string, lines: string[]): string[] {
  const problems: string[] = [];
  lines.forEach((line, index) => {
    try {
      const parsed: unknown = JSON.parse(line);
      if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) throw new Error();
    } catch {
      problems.push(`${label}: line ${index + 1} is not a JSON object`);
    }
  });
  return problems;
}

/** egress.log against expected-egress.txt (008 plan-d decision 10, condition 1 of devops). */
function egressProblems(): string[] {
  // A proxy that never started would switch the guard off silently.
  if (!existsSync(EGRESS_LOG)) return ['egress: egress.log missing (did the proxy start?)'];
  const expected = new Set(fileLines(EXPECTED_EGRESS));
  return fileLines(EGRESS_LOG)
    .filter((target) => !expected.has(target))
    .map((target) => `egress: the browser tried to reach ${target}, which no test declared`);
}

/**
 * Stops the web instances and the server, waits for them to exit and checks the complete log
 * files against the line whitelist and the canaries, so late asynchronous writes cannot hide
 * (plan-a decision 11, 008 plan-d decision 10). Also checks the network guard.
 */
export default async function globalTeardown(): Promise<void> {
  for (const name of PROCESSES) await stop(name);
  const problems: string[] = [];
  const values = canaries();
  for (const instance of INSTANCES) {
    const file = path.join(LOG_DIR, `${instance}.log`);
    if (!existsSync(file)) {
      problems.push(`${instance}: log file missing`);
      continue;
    }
    for (const problem of whitelistProblems(readLines(instance)))
      problems.push(`${instance}: ${problem}`);
    problems.push(...canaryProblems(instance, readLines(instance), values));
  }
  // The instance without catalog must log nothing but its startup.
  const extra = readLines('no-catalog').filter(
    (line) => !line.startsWith('Listening on') && !line.includes('"event":"web_started"'),
  );
  if (extra.length > 0) problems.push(`no-catalog: ${extra.length} request line(s) logged`);

  if (!existsSync(SERVER_LOG)) {
    problems.push('server: log file missing');
  } else {
    const lines = fileLines(SERVER_LOG);
    if (lines.length === 0) problems.push('server: empty log (positive control)');
    problems.push(...jsonProblems('server', lines));
    problems.push(...canaryProblems('server', lines, values));
  }
  problems.push(...canaryProblems('admin-cli', fileLines(ADMIN_CLI_LOG), values));
  problems.push(...egressProblems());

  if (problems.length > 0) {
    throw new Error(`log whitelist failed:\n  ${problems.join('\n  ')}`);
  }
}
