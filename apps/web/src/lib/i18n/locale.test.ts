import { describe, expect, it } from 'vitest';
import {
  isExcludedPath,
  isLocale,
  localeFromAcceptLanguage,
  localizedHref,
  pickLocale,
  serializeLocaleCookie,
} from './locale.js';

describe('isLocale', () => {
  it.each([
    ['es', true],
    ['en', true],
    ['EN', false],
    ['xx', false],
    ['', false],
    [undefined, false],
    ['"><script>', false],
  ])('%s → %s', (value, expected) => {
    expect(isLocale(value)).toBe(expected);
  });
});

describe('pickLocale', () => {
  it('uses a valid cookie over Accept-Language', () => {
    expect(pickLocale('es', 'en-US,en;q=0.9')).toBe('es');
    expect(pickLocale('en', 'es-MX,es;q=0.9')).toBe('en');
  });

  it('ignores an invalid cookie', () => {
    expect(pickLocale('xx', 'es')).toBe('es');
    expect(pickLocale('ES', null)).toBe('en');
    expect(pickLocale('', 'es')).toBe('es');
  });

  it('honours Accept-Language weights and region subtags', () => {
    expect(pickLocale(undefined, 'fr-FR,es;q=0.8,en;q=0.5')).toBe('es');
    expect(pickLocale(undefined, 'en;q=0.4, es-AR;q=0.9')).toBe('es');
    expect(pickLocale(undefined, 'es;q=0, en;q=0.1')).toBe('en');
    expect(pickLocale(undefined, 'de, fr')).toBe('en');
  });

  it('falls back to en without cookie or header', () => {
    expect(pickLocale(undefined, undefined)).toBe('en');
    expect(pickLocale(undefined, null)).toBe('en');
    expect(pickLocale(undefined, '')).toBe('en');
  });

  it('treats an unparseable weight as zero', () => {
    expect(localeFromAcceptLanguage('es;q=abc, en')).toBe('en');
  });
});

describe('isExcludedPath', () => {
  it.each(['/healthz', '/api', '/api/x', '/api/x/y', '/ws', '/ws/', '/healthz/'])(
    '%s is excluded',
    (path) => expect(isExcludedPath(path)).toBe(true),
  );
  it.each(['/', '/apix', '/healthzz', '/wsx', '/es/api', '/en/healthz', '/developers'])(
    '%s is not excluded',
    (path) => expect(isExcludedPath(path)).toBe(false),
  );
});

describe('localizedHref', () => {
  it('swaps the prefix and keeps path and query', () => {
    expect(localizedHref('/es/dev/components', '?theme=light', 'en')).toBe(
      '/en/dev/components?theme=light',
    );
    expect(localizedHref('/en/', '', 'es')).toBe('/es/');
    expect(localizedHref('/en', '', 'es')).toBe('/es/');
  });

  it('never produces a protocol-relative URL', () => {
    for (const path of ['//evil.example/', '/\\evil.example', '/es//evil.example/', '///x']) {
      const href = localizedHref(path, '', 'es');
      expect(href.startsWith('/es/')).toBe(true);
      expect(href.startsWith('/es//')).toBe(false);
      expect(href).not.toMatch(/^\/\/|^\/\\/);
    }
  });

  it('adds the ? to a bare search string', () => {
    expect(localizedHref('/es/x', 'a=1', 'en')).toBe('/en/x?a=1');
  });
});

describe('serializeLocaleCookie', () => {
  it('has the RF-A4 attributes and no HttpOnly', () => {
    const cookie = serializeLocaleCookie('es', 'http:');
    expect(cookie).toBe('locale=es; Path=/; Max-Age=31536000; SameSite=Lax');
    expect(cookie).not.toMatch(/HttpOnly/i);
  });

  it('adds Secure only on https', () => {
    expect(serializeLocaleCookie('en', 'https:')).toBe(
      'locale=en; Path=/; Max-Age=31536000; SameSite=Lax; Secure',
    );
  });
});
