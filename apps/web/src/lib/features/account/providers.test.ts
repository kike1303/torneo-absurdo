import { describe, expect, it } from 'vitest';
import { isProvider, providerRows } from './providers.js';

// 008 RF-C3 and plan-d decision 9: what the account shows for each provider.
describe('providerRows', () => {
  it('lists enabled and linked providers in a fixed order', () => {
    expect(providerRows(['dev'], ['dev', 'google', 'discord']).map((r) => r.provider)).toEqual([
      'google',
      'discord',
      'dev',
    ]);
  });

  it('the only enabled linked provider cannot be unlinked', () => {
    expect(providerRows(['dev'], ['google', 'discord', 'dev'])).toEqual([
      { provider: 'google', linked: false, canUnlink: false },
      { provider: 'discord', linked: false, canUnlink: false },
      { provider: 'dev', linked: true, canUnlink: false },
    ]);
  });

  it('two enabled linked providers can each be unlinked', () => {
    const rows = providerRows(['google', 'dev'], ['google', 'discord', 'dev']);
    expect(rows.filter((r) => r.canUnlink).map((r) => r.provider)).toEqual(['google', 'dev']);
  });

  it('a linked provider that is not enabled does not count as a way to sign in', () => {
    // Discord switched off: unlinking dev would leave only a disabled provider.
    const rows = providerRows(['discord', 'dev'], ['google', 'dev']);
    expect(rows).toEqual([
      { provider: 'google', linked: false, canUnlink: false },
      { provider: 'discord', linked: true, canUnlink: true },
      { provider: 'dev', linked: true, canUnlink: false },
    ]);
  });

  it('without the list of enabled providers, every linked one counts', () => {
    expect(providerRows(['google', 'dev'], null)).toEqual([
      { provider: 'google', linked: true, canUnlink: true },
      { provider: 'dev', linked: true, canUnlink: true },
    ]);
  });
});

describe('isProvider', () => {
  it.each([
    ['google', true],
    ['discord', true],
    ['dev', true],
    ['github', false],
    ['__proto__', false],
    [null, false],
  ])('%j → %s', (value, expected) => expect(isProvider(value)).toBe(expected));
});
