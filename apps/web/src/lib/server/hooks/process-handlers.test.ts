import { afterEach, describe, expect, it, vi } from 'vitest';
import { setLogWriter } from '#lib/server/log.js';
import { installProcessHandlers, logStartup } from './process-handlers.js';

let restore: (() => void) | undefined;
afterEach(() => restore?.());

function fakeProcess() {
  const listeners = new Map<string, (reason: unknown) => void>();
  return {
    listeners,
    on: vi.fn((event: string, listener: (reason: unknown) => void) =>
      listeners.set(event, listener),
    ),
    exit: vi.fn() as unknown as (code: number) => never,
  };
}

describe('installProcessHandlers', () => {
  it('logs uncaught exceptions and exits with 1 after writing', () => {
    const lines: string[] = [];
    restore = setLogWriter((_s, line, done) => {
      lines.push(line);
      done?.();
    });
    const proc = fakeProcess();
    installProcessHandlers(proc);
    proc.listeners.get('uncaughtException')?.(new Error('boom ?q=secret'));
    expect(lines).toHaveLength(1);
    expect(lines[0]).toContain('"event":"uncaught_exception"');
    expect(lines[0]).not.toContain('secret');
    expect(proc.exit).toHaveBeenCalledWith(1);
  });

  it('logs unhandled rejections without exiting', () => {
    const lines: string[] = [];
    restore = setLogWriter((_s, line) => lines.push(line));
    const proc = fakeProcess();
    installProcessHandlers(proc);
    proc.listeners.get('unhandledRejection')?.('reason');
    expect(lines[0]).toContain('"event":"unhandled_rejection"');
    expect(proc.exit).not.toHaveBeenCalled();
  });

  it('installs only once per process', () => {
    const proc = fakeProcess();
    installProcessHandlers(proc);
    installProcessHandlers(proc);
    expect(proc.on).toHaveBeenCalledTimes(2);
  });
});

describe('logStartup', () => {
  it('writes only a code, never environment values', () => {
    const canary = 'http://canary-origin.example:1234';
    vi.stubEnv('PUBLIC_ORIGIN', canary);
    vi.stubEnv('ENABLE_DEV_CATALOG', '1');
    const lines: string[] = [];
    restore = setLogWriter((_s, line) => lines.push(line));
    logStartup();
    vi.unstubAllEnvs();
    expect(lines).toHaveLength(1);
    const parsed = JSON.parse(lines[0]!) as Record<string, unknown>;
    expect(Object.keys(parsed).sort()).toEqual(['event', 'level', 'ts']);
    expect(parsed.event).toBe('web_started');
    expect(lines[0]).not.toContain('canary');
  });
});
