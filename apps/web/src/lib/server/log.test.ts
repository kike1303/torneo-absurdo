import { afterEach, describe, expect, it } from 'vitest';
import { errorFields, formatLogLine, log, setLogWriter } from './log.js';

const ALLOWED = new Set([
  'ts',
  'level',
  'event',
  'error_id',
  'route_id',
  'status',
  'error_name',
  'error_message',
  'stack',
]);

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

  it('sanitizes error_message and stack', () => {
    const line = formatLogLine('error', {
      event: 'x',
      error_message: 'boom at http://h/p?q=secret from 203.0.113.77',
      stack: 'Error: x\n    at f (http://h/a.js?t=1:1:2)',
    });
    expect(line).not.toMatch(/secret|203\.0\.113|t=1/);
  });
});

describe('log', () => {
  it('writes errors to stderr and the rest to stdout', () => {
    const writes: [string, string][] = [];
    restore = setLogWriter((stream, line) => writes.push([stream, line]));
    log('error', { event: 'a' });
    log('info', { event: 'b' });
    expect(writes.map(([s]) => s)).toEqual(['stderr', 'stdout']);
    expect(
      writes.every(([, line]) => line.endsWith('\n') && !line.slice(0, -1).includes('\n')),
    ).toBe(true);
  });
});

describe('errorFields', () => {
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
