import { describe, expect, it } from 'vitest';
import { CodedError, SERVER_ERROR_CODES, isServerErrorCode } from './errors.js';

const EVENT_FORMAT = /^[a-z][a-z0-9_]*(\.[a-z][a-z0-9_]*)*$/;

describe('SERVER_ERROR_CODES', () => {
  it('is non-empty, unique and follows the event format of ADR-0012 A1', () => {
    expect(SERVER_ERROR_CODES.length).toBeGreaterThan(0);
    expect(new Set(SERVER_ERROR_CODES).size).toBe(SERVER_ERROR_CODES.length);
    for (const code of SERVER_ERROR_CODES) expect(code).toMatch(EVENT_FORMAT);
  });
});

describe('isServerErrorCode', () => {
  it('accepts only codes from the closed list', () => {
    for (const code of SERVER_ERROR_CODES) expect(isServerErrorCode(code)).toBe(true);
    for (const value of ['', 'nope', 'Dev.Error_probe', 1, null, undefined, {}]) {
      expect(isServerErrorCode(value)).toBe(false);
    }
  });
});

describe('CodedError', () => {
  it('is an Error with its code and name', () => {
    const code = SERVER_ERROR_CODES[0];
    const error = new CodedError(code);
    expect(error).toBeInstanceOf(Error);
    expect(error.code).toBe(code);
    expect(error.name).toBe('CodedError');
  });
});
