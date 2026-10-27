import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

// Shared by the specs and the global teardown (plan-a decisions 11 and 12).
export const WEB_DIR = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const LOG_DIR = path.join(WEB_DIR, 'test-results', 'logs');
export type Instance = 'catalog' | 'no-catalog';

export const ALLOWED_FIELDS = new Set([
  'ts',
  'level',
  'event',
  'error_id',
  'route_id',
  'status',
  'error_name',
  'error_code',
  'stack',
]);
export const LISTENING = /^Listening on http:\/\/[^\s]+:\d+$/;

/** Values that must never reach a log line (RF-A19, A5 of the intent, ADR-0012 A5). */
export const CANARIES = {
  cookie: 'canary-cookie-5b0c',
  authorization: 'canary-auth-91fe',
  secret: 'canary-secret-3a77',
  clientIp: '198.51.100.23',
  forwardedFor: '203.0.113.77',
  probeIp: '203.0.113.99',
  probeQuery: 'canary-query-7d1e',
  requestQuery: 'canary-request-query-e2c4',
  probeMessage: 'canary-message-0a9f',
  probeName: 'canary-name-b41e',
} as const;

export function logPath(instance: Instance): string {
  return path.join(LOG_DIR, `${instance}.log`);
}

export function readLines(instance: Instance): string[] {
  return readFileSync(logPath(instance), 'utf8')
    .split('\n')
    .filter((line) => line.length > 0);
}

/** Waits for pipes to drain, then returns the lines written since `before`. */
export async function linesSince(
  instance: Instance,
  before: number,
  settleMs = 400,
): Promise<string[]> {
  await new Promise((resolve) => setTimeout(resolve, settleMs));
  return readLines(instance).slice(before);
}

/** Problems of a whole log file against the line whitelist (plan-a decision 12). */
export function whitelistProblems(lines: string[]): string[] {
  const problems: string[] = [];
  let listeningAt = -1;
  let firstRequestLine = -1;
  lines.forEach((line, index) => {
    for (const canary of Object.values(CANARIES)) {
      if (line.includes(canary)) problems.push(`line ${index + 1} contains canary ${canary}`);
    }
    if (LISTENING.test(line)) {
      if (listeningAt !== -1) problems.push(`line ${index + 1}: second "Listening on" line`);
      listeningAt = index;
      return;
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(line);
    } catch {
      problems.push(
        `line ${index + 1} is neither JSON from log.ts nor the Listening line: ${line.slice(0, 120)}`,
      );
      return;
    }
    if (typeof parsed !== 'object' || parsed === null || Array.isArray(parsed)) {
      problems.push(`line ${index + 1} is not a JSON object`);
      return;
    }
    const extra = Object.keys(parsed).filter((key) => !ALLOWED_FIELDS.has(key));
    if (extra.length > 0)
      problems.push(`line ${index + 1} has fields outside RF-A19: ${extra.join(', ')}`);
    const event = (parsed as Record<string, unknown>).event;
    if (event !== 'web_started' && firstRequestLine === -1) firstRequestLine = index;
  });
  if (listeningAt === -1) problems.push('missing "Listening on" line');
  if (firstRequestLine !== -1 && listeningAt > firstRequestLine)
    problems.push('"Listening on" appears after a request line');
  return problems;
}
