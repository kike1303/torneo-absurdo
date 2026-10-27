// Errors as data (002 plan-b decision 3): a code and its data, never text for humans (ADR-0007).
export type ErrorData = Readonly<Record<string, string | number>>;

export class ConverterError extends Error {
  readonly code: string;
  readonly data: ErrorData;

  constructor(code: string, data: ErrorData = {}) {
    super(code);
    this.name = 'ConverterError';
    this.code = code;
    this.data = data;
  }
}

export function fail(code: string, data: ErrorData = {}): never {
  throw new ConverterError(code, data);
}

/** A note for the report: something ignored or adjusted, not an error. */
export interface Note {
  readonly code: string;
  readonly data?: ErrorData;
}
