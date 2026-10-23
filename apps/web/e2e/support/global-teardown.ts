import { existsSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { LOG_DIR, readLines, whitelistProblems, type Instance } from './logs.js';

const INSTANCES: Instance[] = ['catalog', 'no-catalog'];

async function stop(instance: Instance): Promise<void> {
  const pidFile = path.join(LOG_DIR, `${instance}.pid`);
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
  throw new Error(`server ${instance} (pid ${pid}) did not exit`);
}

/**
 * Stops both servers, waits for them to exit and checks the complete log files against the line
 * whitelist and the canaries, so late asynchronous writes cannot hide (plan-a decision 11).
 */
export default async function globalTeardown(): Promise<void> {
  for (const instance of INSTANCES) await stop(instance);
  const problems: string[] = [];
  for (const instance of INSTANCES) {
    const file = path.join(LOG_DIR, `${instance}.log`);
    if (!existsSync(file)) {
      problems.push(`${instance}: log file missing`);
      continue;
    }
    for (const problem of whitelistProblems(readLines(instance)))
      problems.push(`${instance}: ${problem}`);
  }
  // The instance without catalog must log nothing but its startup.
  const extra = readLines('no-catalog').filter(
    (line) => !line.startsWith('Listening on') && !line.includes('"event":"web_started"'),
  );
  if (extra.length > 0) problems.push(`no-catalog: ${extra.length} request line(s) logged`);
  if (problems.length > 0) {
    throw new Error(`log whitelist failed:\n  ${problems.join('\n  ')}`);
  }
}
