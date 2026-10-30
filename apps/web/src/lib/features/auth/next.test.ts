import { describe, expect, it } from 'vitest';
import { NEXT_CASES } from '../../../../tests/support/next-cases.js';
import { safeNext } from './next.js';

describe('safeNext (008 plan-d decision 9)', () => {
  it.each(NEXT_CASES)('%j → %j', (value, expected) => {
    expect(safeNext(value, 'es')).toBe(expected ?? '/es/');
    expect(safeNext(value, 'en')).toBe(expected ?? '/en/');
  });
});
