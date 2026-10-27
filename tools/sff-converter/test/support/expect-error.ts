import { expect } from 'vitest';
import { ConverterError } from '../../src/errors.ts';

/** Runs `fn` and returns the ConverterError it throws (fails the test otherwise). */
export function converterError(fn: () => unknown): ConverterError {
  try {
    fn();
  } catch (error) {
    expect(error).toBeInstanceOf(ConverterError);
    return error as ConverterError;
  }
  throw new Error('expected a ConverterError');
}

export function code(fn: () => unknown): string {
  return converterError(fn).code;
}
