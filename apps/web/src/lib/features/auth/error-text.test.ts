import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';
import {
  CALLBACK_ERROR_TEXT,
  ERROR_TEXT,
  resolveApiError,
  resolveCallbackError,
} from './error-text.js';

// 008 RF-D6, plan-d decision 6 and the condition of architect in plan-a (answer 5). 009 RF24:
// the closed list grows from six to eight.
const CALLBACK = [
  'auth.invalid_state',
  'auth.banned',
  'auth.provider_error',
  'auth.try_again',
  'auth.already_linked',
  'auth.provider_already_linked',
  'auth.reauth_required',
  'auth.reauth_mismatch',
] as const;

describe('resolveCallbackError', () => {
  it.each(CALLBACK)('%s → its own key', (code) => {
    expect(resolveCallbackError(code)).toBe(CALLBACK_ERROR_TEXT[code]);
    expect(resolveCallbackError(code)).not.toBe('error_generic');
  });

  it('without the parameter there is no error', () => {
    expect(resolveCallbackError(null)).toBeNull();
  });

  it.each([
    '',
    '__proto__',
    'constructor',
    'toString',
    'hasOwnProperty',
    // An AuthErrorCode that is not a callback code.
    'auth.csrf_origin',
    'auth.last_provider',
    'AUTH.BANNED',
    ' auth.banned',
    'canary-error-4f2a',
    '<script>alert(1)</script>',
  ])('%j → error_generic', (value) => {
    expect(resolveCallbackError(value)).toBe('error_generic');
  });
});

describe('resolveApiError', () => {
  it('a known code gets its key', () => {
    expect(resolveApiError('auth.last_provider')).toBe(ERROR_TEXT['auth.last_provider']);
    expect(resolveApiError('profile.display_name_taken')).toBe('error_profile_display_name_taken');
    expect(resolveApiError('auth.reauth_mismatch')).toBe('error_auth_reauth_mismatch');
    expect(resolveApiError('auth.reauth_required')).toBe('error_auth_reauth_required');
  });
  it.each([undefined, null, '', '__proto__', 'toString', 'canary', 42])('%j → error_generic', (v) =>
    expect(resolveApiError(v)).toBe('error_generic'),
  );
});

describe('every code has a text in en and es (condition of plan-a, answer 5)', () => {
  const messages = (lang: string) =>
    JSON.parse(
      readFileSync(new URL(`../../../../messages/${lang}.json`, import.meta.url), 'utf8'),
    ) as Record<string, string>;
  const en = messages('en');
  const es = messages('es');
  const keys = new Set([
    ...Object.values(ERROR_TEXT),
    ...Object.values(CALLBACK_ERROR_TEXT),
    'error_generic',
  ]);

  it.each([...keys])('%s', (key) => {
    expect(en[key], `en ${key}`).toBeTruthy();
    expect(es[key], `es ${key}`).toBeTruthy();
    expect(en[key]!.trim()).not.toBe('');
    expect(es[key]!.trim()).not.toBe('');
    expect(en[key], `${key} is not translated`).not.toBe(es[key]);
  });

  it('every code of the protocol is in the table', () => {
    // 4 + 11 + 11 + 3 + 2 (ApiErrorCode, WsErrorCode, AuthErrorCode, ProfileErrorCode, FightErrorCode).
    expect(Object.keys(ERROR_TEXT)).toHaveLength(31);
    expect(Object.keys(CALLBACK_ERROR_TEXT)).toEqual([...CALLBACK]);
  });

  it('ws.* reuse the ws_close_reason_* texts', () => {
    for (const [code, key] of Object.entries(ERROR_TEXT)) {
      if (code.startsWith('ws.')) expect(key).toMatch(/^ws_close_reason_/);
      else expect(key).toBe(`error_${code.replace('.', '_')}`);
    }
  });
});
