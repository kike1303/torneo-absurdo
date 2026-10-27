// Helpers for validating untrusted JSON (plan-a1 decision 14): only own keys of plain objects,
// no recursion over the input, and a hard cap on the number of errors.

export interface InputError<C extends string> {
  readonly code: C;
  readonly path: string;
  readonly data?: Readonly<Record<string, string | number>>;
}

/** Thrown by `ErrorSink.add` once the cap is reached; caught by the validators. */
export class StopValidation {
  readonly stop = true;
}

export class ErrorSink<C extends string> {
  readonly errors: InputError<C>[] = [];
  private readonly max: number;
  private readonly tooMany: C;

  constructor(max: number, tooMany: C) {
    this.max = max;
    this.tooMany = tooMany;
  }

  add(code: C, path: string, data?: Record<string, string | number>): void {
    if (this.errors.length >= this.max) {
      this.errors.push({ code: this.tooMany, path: '' });
      throw new StopValidation();
    }
    this.errors.push(data === undefined ? { code, path } : { code, path, data });
  }

  get count(): number {
    return this.errors.length;
  }
}

export function isPlainObject(value: unknown): value is Record<string, unknown> {
  if (typeof value !== 'object' || value === null || Array.isArray(value)) return false;
  const proto: unknown = Object.getPrototypeOf(value);
  return proto === Object.prototype || proto === null;
}

/** Own property read that never walks the prototype chain. */
export function own(obj: Record<string, unknown>, key: string): unknown {
  return Object.hasOwn(obj, key) ? obj[key] : undefined;
}

/** JSON Pointer (RFC 6901) from path segments. */
export function pointer(...segments: readonly (string | number)[]): string {
  return segments.map((s) => '/' + String(s).replaceAll('~', '~0').replaceAll('/', '~1')).join('');
}

export function isInt(value: unknown): value is number {
  return typeof value === 'number' && Number.isInteger(value);
}

export function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}
