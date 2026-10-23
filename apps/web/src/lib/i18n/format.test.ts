import { describe, expect, it } from 'vitest';
import { formatCoins, formatDate, formatRelative } from './format.js';

describe('format', () => {
  it('formats coins with the locale grouping', () => {
    expect(formatCoins(1234567, 'en')).toBe('1,234,567');
    expect(formatCoins(1234567, 'es')).toBe('1.234.567');
  });

  it('formats dates per locale', () => {
    const date = new Date(Date.UTC(2026, 9, 22, 12, 0));
    expect(formatDate(date, 'en', { dateStyle: 'long', timeZone: 'UTC' })).toBe('October 22, 2026');
    expect(formatDate(date, 'es', { dateStyle: 'long', timeZone: 'UTC' })).toBe(
      '22 de octubre de 2026',
    );
  });

  it('formats relative times with the largest unit', () => {
    const now = new Date(Date.UTC(2026, 0, 10));
    expect(formatRelative(new Date(Date.UTC(2026, 0, 8)), 'en', now)).toBe('2 days ago');
    expect(formatRelative(new Date(Date.UTC(2026, 0, 7)), 'es', now)).toBe('hace 3 días');
    expect(formatRelative(new Date(Date.UTC(2026, 0, 8)), 'es', now)).toBe('anteayer');
    expect(formatRelative(new Date(now.getTime() + 5 * 60_000), 'en', now)).toBe('in 5 minutes');
  });
});
