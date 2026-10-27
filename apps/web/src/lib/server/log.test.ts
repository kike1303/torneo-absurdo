import assert from 'node:assert';
import { afterEach, describe, expect, it } from 'vitest';
import { CodedError, SERVER_ERROR_CODES } from './errors.js';
import { errorFields, formatLogLine, log, setLogWriter, type LogFields } from './log.js';

const ALLOWED = new Set([
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

const CANARY = 'canary-msg-6c1d';

let restore: (() => void) | undefined;
afterEach(() => restore?.());

describe('formatLogLine', () => {
  it('emits one JSON line with only the allowed fields', () => {
    const fields = {
      event: 'x',
      status: 500,
      url: 'http://h/?q=1',
      cookie: 'a=b',
    } as unknown as Parameters<typeof formatLogLine>[1];
    const parsed = JSON.parse(formatLogLine('error', fields, new Date(0))) as Record<
      string,
      unknown
    >;
    expect(Object.keys(parsed).every((k) => ALLOWED.has(k))).toBe(true);
    expect(parsed).toEqual({
      ts: '1970-01-01T00:00:00.000Z',
      level: 'error',
      event: 'x',
      status: 500,
    });
  });

  it('sanitizes stack', () => {
    const line = formatLogLine('error', {
      event: 'unhandled_error',
      stack: '    at f (http://h/a.js?t=1:1:2) 203.0.113.77',
    });
    expect(line).not.toMatch(/203\.0\.113|t=1/);
  });

  it('keeps only frame lines of a raw stack passed by a caller', () => {
    const line = formatLogLine('error', {
      event: 'unhandled_error',
      stack: new Error(`raw ${CANARY}`).stack!,
    });
    expect(line).not.toContain(CANARY);
    expect(JSON.parse(line).stack).toMatch(/^ +at /);
    const frameless = formatLogLine('error', {
      event: 'unhandled_error',
      stack: `Error: ${CANARY}`,
    });
    expect(frameless).not.toContain('stack');
  });

  it('drops fields with the wrong runtime type or format', () => {
    const fields = {
      event: `free text ${CANARY}`,
      error_id: `not-a-uuid-${CANARY}`,
      route_id: { toString: () => CANARY },
      status: 500.5,
    } as unknown as LogFields;
    const parsed = JSON.parse(formatLogLine('error', fields, new Date(0)));
    expect(parsed).toEqual({ ts: '1970-01-01T00:00:00.000Z', level: 'error' });
    const valid = JSON.parse(
      formatLogLine('error', {
        event: 'unhandled_error',
        error_id: '0b9c7c1e-2f43-4c1a-9a43-7d1f3e0b2a11',
        route_id: null,
        status: 500,
      }),
    );
    expect(valid).toMatchObject({ event: 'unhandled_error', route_id: null, status: 500 });
    expect(valid.error_id).toBe('0b9c7c1e-2f43-4c1a-9a43-7d1f3e0b2a11');
  });

  it('never writes error_message, even if a caller passes it', () => {
    const fields = {
      event: 'unhandled_error',
      error_message: `boom ${CANARY}`,
    } as unknown as LogFields;
    const line = formatLogLine('error', fields);
    expect(line).not.toContain('error_message');
    expect(line).not.toContain(CANARY);
  });

  it.each([
    ['TypeError', 'TypeError'],
    ['$weird_Name1', '$weird_Name1'],
    [`canary-${CANARY}`, 'other'],
    ['has space', 'other'],
    ['', 'other'],
    ['A'.repeat(65), 'other'],
    ['1Error', 'other'],
  ])('writes error_name %j as %j', (name, expected) => {
    const parsed = JSON.parse(
      formatLogLine('error', { event: 'unhandled_error', error_name: name }),
    );
    expect(parsed.error_name).toBe(expected);
  });

  it('writes error_code only when it is in SERVER_ERROR_CODES', () => {
    const code = SERVER_ERROR_CODES[0];
    expect(
      JSON.parse(formatLogLine('error', { event: 'unhandled_error', error_code: code })),
    ).toHaveProperty('error_code', code);
    const forged = { event: 'unhandled_error', error_code: CANARY } as unknown as LogFields;
    expect(formatLogLine('error', forged)).not.toContain(CANARY);
  });

  it('types event as a closed union', () => {
    // @ts-expect-error: free text is not a LogEvent (ADR-0012 trabajo 2).
    const fields: LogFields = { event: 'anything goes' };
    expect(fields.event).toBe('anything goes');
  });
});

describe('log', () => {
  it('writes errors to stderr and the rest to stdout', () => {
    const writes: [string, string][] = [];
    restore = setLogWriter((stream, line) => writes.push([stream, line]));
    log('error', { event: 'unhandled_error' });
    log('info', { event: 'web_started' });
    expect(writes.map(([s]) => s)).toEqual(['stderr', 'stdout']);
    expect(
      writes.every(([, line]) => line.endsWith('\n') && !line.slice(0, -1).includes('\n')),
    ).toBe(true);
  });
});

describe('errorFields', () => {
  it('never returns the message and strips it from the stack header', () => {
    const fields = errorFields(new Error(`boom ${CANARY}`));
    expect(fields).not.toHaveProperty('error_message');
    expect(fields.error_name).toBe('Error');
    expect(fields.stack).toMatch(/^ +at /);
    expect(JSON.stringify(fields)).not.toContain(CANARY);
  });

  it('strips a multi-line message that imitates frames when the header matches', () => {
    const fields = errorFields(new Error(`boom\n    at fake (${CANARY})`));
    expect(fields.stack).toMatch(/^\s+at /);
    expect(JSON.stringify(fields)).not.toContain(CANARY);
  });

  it('cuts everything before the first frame when the header does not match', () => {
    const error = new Error(`first ${CANARY}`);
    // V8 builds the stack text on first read: read it before mutating, so the header is stale.
    void error.stack;
    error.message = 'changed later';
    error.name = `canary-${CANARY}`;
    const fields = errorFields(error);
    expect(fields.error_name).toBe('other');
    expect(fields.stack).toMatch(/^\s+at /);
    expect(JSON.stringify(fields)).not.toContain(CANARY);
  });

  it('cuts the header of a Node internal error (`Name [ERR_X]: message`)', () => {
    let thrown: unknown;
    try {
      assert.strictEqual(`value-${CANARY}`, 'other');
    } catch (error) {
      thrown = error;
    }
    const fields = errorFields(thrown);
    expect(fields.error_name).toBe('AssertionError');
    expect(fields.stack).toMatch(/^ +at /);
    expect(JSON.stringify(fields)).not.toContain(CANARY);
    const synthetic = {
      name: 'TypeError',
      message: 'm',
      stack: `TypeError [ERR_X]: ${CANARY}\n    at f (a.js:1:2)`,
    };
    expect(errorFields(synthetic)).toEqual({
      error_name: 'TypeError',
      stack: '    at f (a.js:1:2)',
    });
  });

  it('keeps only frame lines, not text after the first frame', () => {
    const fields = errorFields({
      name: 'Error',
      message: 'm',
      stack: `Error: other\n    at f (a.js:1:2)\n${CANARY} tail\n    at g (b.js:3:4)`,
    });
    expect(fields.stack).toBe('    at f (a.js:1:2)\n    at g (b.js:3:4)');
  });

  it('omits stack without any frame line', () => {
    expect(errorFields({ name: 'Error', stack: `Error: ${CANARY}` })).toEqual({
      error_name: 'Error',
    });
    expect(errorFields({ name: 'Error', stack: 42 })).toEqual({ error_name: 'Error' });
    const previous = Error.stackTraceLimit;
    Error.stackTraceLimit = 0;
    try {
      const fields = errorFields(new Error(`no frames ${CANARY}`));
      expect(fields).toEqual({ error_name: 'Error' });
    } finally {
      Error.stackTraceLimit = previous;
    }
  });

  it('logs a CodedError by its code only', () => {
    const code = SERVER_ERROR_CODES[0];
    expect(errorFields(new CodedError(code))).toEqual({ error_code: code });
  });

  it('treats a CodedError with a forged code as any other error', () => {
    const error = new CodedError(SERVER_ERROR_CODES[0]);
    Object.defineProperty(error, 'code', { value: CANARY });
    const fields = errorFields(error);
    expect(fields).not.toHaveProperty('error_code');
    expect(fields.error_name).toBe('CodedError');
    expect(JSON.stringify(fields)).not.toContain(CANARY);
  });

  it('ignores an object that only looks like a CodedError', () => {
    const fields = errorFields({ name: 'CodedError', code: SERVER_ERROR_CODES[0] });
    expect(fields).toEqual({ error_name: 'CodedError' });
  });

  it('propagates throwing getters when asked to', () => {
    const evil = {
      get stack(): string {
        throw new Error('getter');
      },
    };
    expect(() => errorFields(evil, { propagate: true })).toThrow('getter');
  });

  it('survives throwing getters and non-objects', () => {
    const evil = {
      get message(): string {
        throw new Error('getter');
      },
      name: 'Evil',
    };
    expect(errorFields(evil)).toEqual({ error_name: 'Evil' });
    expect(errorFields('text')).toEqual({ error_name: 'string' });
  });
});
