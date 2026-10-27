import type { HandleServerError } from '@sveltejs/kit/hooks';
import { errorFields, log } from '#lib/server/log.js';

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
    // A throwing getter propagates to the `catch`: nothing is logged.
    const fields = errorFields(input.error, { propagate: true });
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
