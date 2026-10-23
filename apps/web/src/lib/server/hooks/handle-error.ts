import type { HandleServerError } from '@sveltejs/kit/hooks';
import { log, type LogFields } from '#lib/server/log.js';

/** Closed list of messages sent to the client (plan-a decision 4). Never `error.message`. */
export type ErrorCode = 'not_found' | 'internal';

export interface HandledError {
  message: ErrorCode;
  errorId?: string;
}

type HandleErrorInput = Parameters<HandleServerError>[0];

function statusOf(input: HandleErrorInput): number {
  if (input.kind === 'unknown') return 500;
  const status: unknown = input.error.status;
  return typeof status === 'number' ? status : 500;
}

/** Reads the error's fields. Any throwing getter propagates to the caller's `catch`. */
function readError(error: unknown): Pick<LogFields, 'error_name' | 'error_message' | 'stack'> {
  if (typeof error !== 'object' || error === null) return { error_name: typeof error };
  const { name, message, stack } = error as { name?: unknown; message?: unknown; stack?: unknown };
  return {
    ...(typeof name === 'string' ? { error_name: name } : {}),
    ...(typeof message === 'string' ? { error_message: message } : {}),
    ...(typeof stack === 'string' ? { stack } : {}),
  };
}

/**
 * Server `handleError` (RF-A19, plan-a decision 4). Synchronous and never throws:
 * - only `kind: 'unknown'` is logged, with a fresh `error_id` that is also returned as `errorId`;
 * - `message` is a fixed code; `status` is not returned, so Kit keeps the one it computed;
 * - if anything fails (e.g. a throwing getter), it returns `{ message: 'internal' }` and logs nothing.
 */
export function handleError(input: HandleErrorInput): HandledError {
  try {
    const status = statusOf(input);
    const message: ErrorCode = status === 404 ? 'not_found' : 'internal';
    if (input.kind !== 'unknown') return { message };
    const fields = readError(input.error);
    const errorId = crypto.randomUUID();
    log('error', {
      event: 'unhandled_error',
      error_id: errorId,
      route_id: input.event.route.id,
      status,
      ...fields,
    });
    return { message, errorId };
  } catch {
    return { message: 'internal' };
  }
}
