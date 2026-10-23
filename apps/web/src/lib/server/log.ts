import { sanitize } from './sanitize.js';

// The only writer of logs in `web` (RF-A19). One JSON object per line, only the allowed fields.
// `web` never logs requests: only startup, `handleError` (kind `unknown`) and process errors.

export type LogLevel = 'info' | 'warn' | 'error';

export interface LogFields {
  /** A fixed code, never free text. */
  event: string;
  error_id?: string;
  /** `event.route.id` (the route template), never the URL. */
  route_id?: string | null;
  status?: number;
  error_name?: string;
  error_message?: string;
  stack?: string;
}

const ALLOWED_FIELDS = [
  'event',
  'error_id',
  'route_id',
  'status',
  'error_name',
  'error_message',
  'stack',
] as const satisfies readonly (keyof LogFields)[];

/** Fields that may carry text from an error, always passed through the sanitizer. */
const SANITIZED_FIELDS = new Set<keyof LogFields>(['error_message', 'stack']);

export function formatLogLine(level: LogLevel, fields: LogFields, now: Date = new Date()): string {
  const line: Record<string, unknown> = { ts: now.toISOString(), level };
  for (const key of ALLOWED_FIELDS) {
    const value = fields[key];
    if (value === undefined) continue;
    line[key] = SANITIZED_FIELDS.has(key) && typeof value === 'string' ? sanitize(value) : value;
  }
  return JSON.stringify(line);
}

export type LogWriter = (stream: 'stdout' | 'stderr', line: string, done?: () => void) => void;

const processWriter: LogWriter = (stream, line, done) => {
  const target = stream === 'stderr' ? process.stderr : process.stdout;
  if (done) target.write(line, () => done());
  else target.write(line);
};

let writer: LogWriter = processWriter;

/** Test hook: replaces the writer and returns a function that restores the previous one. */
export function setLogWriter(next: LogWriter): () => void {
  const previous = writer;
  writer = next;
  return () => {
    writer = previous;
  };
}

/** Writes one line. `done` runs after the line was handed to the stream (used before exiting). */
export function log(level: LogLevel, fields: LogFields, done?: () => void): void {
  writer(level === 'error' ? 'stderr' : 'stdout', `${formatLogLine(level, fields)}\n`, done);
}

/** Error fields from an unknown thrown value. Reads each property defensively. */
export function errorFields(
  error: unknown,
): Pick<LogFields, 'error_name' | 'error_message' | 'stack'> {
  const fields: Pick<LogFields, 'error_name' | 'error_message' | 'stack'> = {};
  if (typeof error !== 'object' || error === null) {
    fields.error_name = typeof error;
    return fields;
  }
  const name = readString(error, 'name');
  const message = readString(error, 'message');
  const stack = readString(error, 'stack');
  if (name !== undefined) fields.error_name = name;
  if (message !== undefined) fields.error_message = message;
  if (stack !== undefined) fields.stack = stack;
  return fields;
}

function readString(target: object, key: string): string | undefined {
  try {
    const value: unknown = Reflect.get(target, key);
    return typeof value === 'string' ? value : undefined;
  } catch {
    return undefined;
  }
}
