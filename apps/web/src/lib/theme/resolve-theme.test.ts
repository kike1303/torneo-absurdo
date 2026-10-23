import { describe, expect, it } from 'vitest';
import { parseTheme, resolveTheme, serializeThemeCookie, themeMeta } from './resolve-theme.js';

describe('resolveTheme (cookie table)', () => {
  it.each([
    [undefined, 'dark'],
    ['dark', 'dark'],
    ['light', 'light'],
    ['system', 'system'],
    ['DARK', 'dark'],
    ['"><script>alert(1)</script>', 'dark'],
    ['', 'dark'],
  ])('cookie %j → %s', (cookie, expected) => {
    expect(
      resolveTheme({ cookie, override: null, enableDevCatalog: undefined, routeId: '/' }),
    ).toBe(expected);
  });

  it('never returns a value outside the list', () => {
    for (const cookie of ['x', 'system ', 'light;', '%theme%']) {
      expect(['dark', 'light', 'system']).toContain(
        resolveTheme({ cookie, override: null, enableDevCatalog: '1', routeId: '/dev/components' }),
      );
    }
  });
});

describe('resolveTheme (catalog override, RF-A9)', () => {
  const base = {
    cookie: 'dark',
    enableDevCatalog: '1' as string | undefined,
    routeId: '/dev/components' as string | null,
  };
  it('applies ?theme=light|dark on /dev/** with the catalog on', () => {
    expect(resolveTheme({ ...base, override: 'light' })).toBe('light');
    expect(resolveTheme({ ...base, cookie: 'light', override: 'dark' })).toBe('dark');
  });
  it('ignores the override elsewhere, without the variable, or with other values', () => {
    expect(resolveTheme({ ...base, override: 'light', routeId: '/' })).toBe('dark');
    expect(resolveTheme({ ...base, override: 'light', routeId: '/developers' })).toBe('dark');
    expect(resolveTheme({ ...base, override: 'light', routeId: null })).toBe('dark');
    expect(resolveTheme({ ...base, override: 'light', enableDevCatalog: undefined })).toBe('dark');
    expect(resolveTheme({ ...base, override: 'light', enableDevCatalog: '0' })).toBe('dark');
    expect(resolveTheme({ ...base, override: 'system' })).toBe('dark');
    expect(resolveTheme({ ...base, override: '"><x>' })).toBe('dark');
  });
});

describe('themeMeta', () => {
  it('emits one theme-color for dark and light', () => {
    expect(themeMeta('dark')).toContain('content="dark"');
    expect(themeMeta('light').match(/theme-color/g)).toHaveLength(1);
  });
  it('emits light dark and two media theme-colors for system', () => {
    const meta = themeMeta('system');
    expect(meta).toContain('content="light dark"');
    expect(meta.match(/name="theme-color" media=/g)).toHaveLength(2);
  });
});

describe('parseTheme and the cookie', () => {
  it('validates against the list', () => {
    expect(parseTheme('light')).toBe('light');
    expect(parseTheme('Light')).toBeUndefined();
  });
  it('serializes with the D3 attributes', () => {
    expect(serializeThemeCookie('light', 'http:')).toBe(
      'theme=light; Path=/; Max-Age=31536000; SameSite=Lax',
    );
    expect(serializeThemeCookie('dark', 'https:')).toMatch(/; Secure$/);
  });
});
