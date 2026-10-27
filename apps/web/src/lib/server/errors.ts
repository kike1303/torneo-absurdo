// Errors of our own that are logged by a code from a closed list, never by their message
// (ADR-0012 A5). Not to be confused with `ErrorCode` in `handle-error.ts`, which is what the client
// sees and does not change.

/**
 * Closed list of server error codes, in the `event` format of ADR-0012 A1. A feature adds the code
 * it throws in its own PR (A6).
 */
export const SERVER_ERROR_CODES = ['internal.invariant'] as const;

export type ServerErrorCode = (typeof SERVER_ERROR_CODES)[number];

const CODES: ReadonlySet<unknown> = new Set(SERVER_ERROR_CODES);

export function isServerErrorCode(value: unknown): value is ServerErrorCode {
  return CODES.has(value);
}

/** An error logged only by its `code`. Its message, if any, never reaches a log line. */
export class CodedError extends Error {
  readonly code: ServerErrorCode;

  constructor(code: ServerErrorCode, options?: ErrorOptions) {
    super(code, options);
    this.name = 'CodedError';
    this.code = code;
  }
}
