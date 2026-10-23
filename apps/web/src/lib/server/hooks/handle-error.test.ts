import { error as kitError, isHttpError } from '@sveltejs/kit';
import { afterEach, beforeEach, describe, expect, it } from 'vitest';
import { setLogWriter } from '#lib/server/log.js';
import { fakeEvent } from '../../../../tests/support/event.js';
import { handleError } from './handle-error.js';

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
const CANARY = 'canary-7f3a';

let lines: string[];
let restore: () => void;
beforeEach(() => {
  lines = [];
  restore = setLogWriter((_stream, line) => lines.push(line));
});
afterEach(() => restore());

const event = fakeEvent({ url: 'http://h/es/dev/error-probe?q=x', routeId: '/dev/error-probe' });

function appError(status: number): { status: number; message: string } {
  try {
    kitError(status, `app ${CANARY}`);
  } catch (e) {
    if (isHttpError(e)) return { status: e.status, message: e.body.message };
  }
  throw new Error('unreachable');
}

type Input = Parameters<typeof handleError>[0];
const inputs: Record<string, Input> = {
  unknown: {
    kind: 'unknown',
    error: new Error(`boom http://x.example/p?q=${CANARY} 203.0.113.77`),
    event,
  },
  app: { kind: 'app', error: appError(404), event } as Input,
  framework: { kind: 'framework', error: { status: 404, message: 'Not Found' }, event },
  validation: {
    kind: 'validation',
    error: { status: 400, message: 'Bad Request' },
    issues: [],
    event,
  } as Input,
};

describe('handleError', () => {
  it('logs only kind unknown, with allowed fields and the returned errorId', () => {
    const result = handleError(inputs.unknown!);
    expect(lines).toHaveLength(1);
    const parsed = JSON.parse(lines[0]!) as Record<string, unknown>;
    expect(Object.keys(parsed).every((k) => ALLOWED.has(k))).toBe(true);
    expect(parsed.error_id).toBe(result.errorId);
    expect(parsed.route_id).toBe('/dev/error-probe');
    expect(parsed.status).toBe(500);
    expect(lines[0]).not.toContain(CANARY);
    expect(lines[0]).not.toContain('203.0.113.77');
    expect(result).toEqual({ message: 'internal', errorId: expect.any(String) });
  });

  it.each(['app', 'framework', 'validation'])(
    'kind %s writes nothing and has no errorId',
    (kind) => {
      const result = handleError(inputs[kind]!);
      expect(lines).toHaveLength(0);
      expect(result.errorId).toBeUndefined();
    },
  );

  it('maps messages to the closed list and never returns status or the error text', () => {
    expect(handleError(inputs.app!)).toEqual({ message: 'not_found' });
    expect(handleError(inputs.framework!)).toEqual({ message: 'not_found' });
    expect(handleError(inputs.validation!)).toEqual({ message: 'internal' });
    for (const input of Object.values(inputs)) {
      const result = handleError(input) as unknown as Record<string, unknown>;
      expect(['not_found', 'internal']).toContain(result.message);
      expect(result).not.toHaveProperty('status');
      expect(JSON.stringify(result)).not.toContain(CANARY);
    }
  });

  it('does not throw with a throwing message getter and logs nothing', () => {
    const evil = Object.create(Error.prototype, {
      message: {
        get(): string {
          throw new Error(`getter ${CANARY}`);
        },
      },
    }) as Error;
    const result = handleError({ kind: 'unknown', error: evil, event });
    expect(result).toEqual({ message: 'internal' });
    expect(lines).toHaveLength(0);
  });

  it('is synchronous for every kind', () => {
    expect(handleError.constructor.name).not.toBe('AsyncFunction');
    for (const input of Object.values(inputs)) {
      const result = handleError(input) as unknown as { then?: unknown };
      expect(result).not.toBeInstanceOf(Promise);
      expect(typeof result?.then).not.toBe('function');
    }
  });
});
