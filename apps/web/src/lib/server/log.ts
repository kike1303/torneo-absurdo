import { CodedError, isServerErrorCode, type ServerErrorCode } from './errors.js';
import { sanitize } from './sanitize.js';

// The only writer of logs in `web` (RF-A19, ADR-0012 part A). One JSON object per line, only the
// allowed fields. `web` never logs requests: only startup, `handleError` (kind `unknown`) and
// process errors.

export type LogLevel = 'info' | 'warn' | 'error';

/** Closed list of events (ADR-0012 A1 and trabajo 2). A new one is added in the feature's PR (A6). */
export type LogEvent =
  'web_started' | 'unhandled_error' | 'unhandled_rejection' | 'uncaught_exception';

export interface LogFields {
  event: LogEvent;
  error_id?: string;
  /** `event.route.id` (the route template), never the URL. */
  route_id?: string | null;
  status?: number;
  /** Written only if it is an identifier; otherwise `other` (ADR-0012 A5). */
  error_name?: string;
  /** Code of a `CodedError`; never its message (ADR-0012 A5). */
  error_code?: ServerErrorCode;
  /** Frames only: without the `Name: message` header (ADR-0012 A5). */
  stack?: string;
}

const ALLOWED_FIELDS = [
  'event',
  'error_id',
  'route_id',
  'status',
  'error_name',
  'error_code',
  'stack',
] as const satisfies readonly (keyof LogFields)[];

const EVENT = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const ERROR_NAME = /^[A-Za-z_$][A-Za-z0-9_$]{0,63}$/;
const OTHER_NAME = 'other';
/** A V8 stack frame line. */
const FRAME_LINE = /^[ \t]+at /;

/**
 * Per-field runtime guards (ADR-0012 A3): the value that is written, or `undefined` to drop the
 * field. They hold even when a caller casts its way past `LogFields`.
 */
const GUARDS: Record<(typeof ALLOWED_FIELDS)[number], (value: unknown) => unknown> = {
  event: (value) => (typeof value === 'string' && EVENT.test(value) ? value : undefined),
  error_id: (value) => (typeof value === 'string' && UUID.test(value) ? value : undefined),
  route_id: (value) => (value === null || typeof value === 'string' ? value : undefined),
  status: (value) => (Number.isInteger(value) ? value : undefined),
  error_name: (value) => (typeof value === 'string' && ERROR_NAME.test(value) ? value : OTHER_NAME),
  error_code: (value) => (isServerErrorCode(value) ? value : undefined),
  stack: (value) => {
    const frames = typeof value === 'string' ? frameLines(value) : undefined;
    // Defense in depth: the allowlist and ADR-0012 A5 are the guarantee, not the sanitizer.
    return frames === undefined ? undefined : sanitize(frames);
  },
};

export function formatLogLine(level: LogLevel, fields: LogFields, now: Date = new Date()): string {
  const line: Record<string, unknown> = { ts: now.toISOString(), level };
  for (const key of ALLOWED_FIELDS) {
    const raw: unknown = fields[key];
    if (raw === undefined) continue;
    const value = GUARDS[key](raw);
    if (value !== undefined) line[key] = value;
  }
  return JSON.stringify(line);
}

/** Only the frame lines of a stack (ADR-0012 A5), or `undefined` if it has none. */
function frameLines(stack: string): string | undefined {
  const frames = stack.split('\n').filter((line) => FRAME_LINE.test(line));
  return frames.length > 0 ? frames.join('\n') : undefined;
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

export type ErrorFields = Pick<LogFields, 'error_name' | 'error_code' | 'stack'>;

export interface ErrorFieldsOptions {
  /** Let a throwing getter (or `instanceof` trap) propagate instead of skipping that property. */
  propagate?: boolean;
}

/**
 * Error fields from an unknown thrown value (ADR-0012 A5). Never the message: a `CodedError` with
 * a code from the closed list gives only `error_code`; anything else gives `error_name` and the
 * stack frames. Values that are not objects give their `typeof`.
 */
export function errorFields(error: unknown, options: ErrorFieldsOptions = {}): ErrorFields {
  if (typeof error !== 'object' || error === null) return { error_name: typeof error };
  const guard = <T>(read: () => T): T | undefined => {
    if (options.propagate) return read();
    try {
      return read();
    } catch {
      return undefined;
    }
  };
  const read = (key: string): unknown => guard(() => Reflect.get(error, key) as unknown);

  if (guard(() => error instanceof CodedError)) {
    const code = read('code');
    if (isServerErrorCode(code)) return { error_code: code };
  }

  const name = read('name');
  const fields: ErrorFields = {
    error_name: typeof name === 'string' && ERROR_NAME.test(name) ? name : OTHER_NAME,
  };
  const stack = stackFrames(name, read('message'), read('stack'));
  if (stack !== undefined) fields.stack = stack;
  return fields;
}

/**
 * The stack without its `Name: message` header: the exact header is removed first (it may span
 * lines that imitate frames) and then every line that is not a frame. Without frames there is no
 * stack.
 */
function stackFrames(name: unknown, message: unknown, stack: unknown): string | undefined {
  if (typeof stack !== 'string') return undefined;
  let rest = stack;
  if (typeof name === 'string' && typeof message === 'string') {
    const header = message === '' ? name : `${name}: ${message}`;
    if (rest.startsWith(header)) rest = rest.slice(header.length);
  }
  return frameLines(rest);
}
