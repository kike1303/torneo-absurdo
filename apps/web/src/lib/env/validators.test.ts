import { describe, expect, it } from 'vitest';
import { validateEnableDevCatalog, validatePublicOrigin } from './validators.js';

describe('validatePublicOrigin', () => {
  it.each([
    'http://localhost:5173',
    'https://torneo.example',
    'http://127.0.0.1:4173',
    'https://torneo.example:8443',
  ])('accepts %s', (value) => expect(validatePublicOrigin(value, false)).toBe(value));

  const invalid = [
    'https://torneo.example/es',
    'https://torneo.example/',
    'https://torneo.example?x=1',
    'https://torneo.example#x',
    'https://user:pass@torneo.example',
    'ftp://torneo.example',
    'https://Torneo.example',
    'https://torneo.example:443',
    'http://torneo.example:80',
    'torneo.example',
    'not a url',
  ];
  it.each(invalid)('rejects %s without echoing it', (value) => {
    let message = '';
    expect(() => {
      try {
        validatePublicOrigin(value, false);
      } catch (e) {
        message = (e as Error).message;
        throw e;
      }
    }).toThrow();
    expect(message).toMatch(/^PUBLIC_ORIGIN /);
    expect(message).not.toContain(value);
  });

  it('is required at runtime and optional while building', () => {
    expect(() => validatePublicOrigin(undefined, false)).toThrow(/required/);
    expect(() => validatePublicOrigin('', false)).toThrow(/required/);
    expect(validatePublicOrigin(undefined, true)).toBeUndefined();
  });
});

describe('validateEnableDevCatalog', () => {
  it.each([
    ['1', '1'],
    ['0', undefined],
    ['true', undefined],
    ['', undefined],
    [undefined, undefined],
  ] as const)('%j → %j', (value, expected) =>
    expect(validateEnableDevCatalog(value)).toBe(expected),
  );
});
