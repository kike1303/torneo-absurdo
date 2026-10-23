import { errorFields, log } from '#lib/server/log.js';

const INSTALLED = Symbol.for('torneo.web.processHandlers');

interface ProcessLike {
  on(
    event: 'unhandledRejection' | 'uncaughtException',
    listener: (reason: unknown) => void,
  ): unknown;
  exit(code: number): never;
}

/**
 * RF-A19: logs unhandled rejections and uncaught exceptions through `log.ts`. After an uncaught
 * exception it exits with code 1 (Docker restarts the container) once the line was written.
 * Installed once per process, even if `init` runs again (Vite HMR).
 */
export function installProcessHandlers(proc: ProcessLike = process): void {
  const flags = proc as unknown as Record<symbol, boolean>;
  if (flags[INSTALLED]) return;
  flags[INSTALLED] = true;
  proc.on('unhandledRejection', (reason) => {
    log('error', { event: 'unhandled_rejection', ...errorFields(reason) });
  });
  proc.on('uncaughtException', (error) => {
    log('error', { event: 'uncaught_exception', ...errorFields(error) }, () => proc.exit(1));
  });
}

/** Startup line: only a code, never values of environment variables (plan-a decision 12). */
export function logStartup(): void {
  log('info', { event: 'web_started' });
}
