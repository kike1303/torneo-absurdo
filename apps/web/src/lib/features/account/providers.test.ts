import { describe, expect, it } from 'vitest';
import type { ReauthStatus } from '@torneo/protocol';
import { isProvider, linkPanel, parseReauthStatus, providerRows } from './providers.js';

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
      { provider: 'google', linked: false, canUnlink: false, reauthWith: [], blocked: false },
      { provider: 'discord', linked: false, canUnlink: false, reauthWith: [], blocked: false },
      { provider: 'dev', linked: true, canUnlink: false, reauthWith: ['dev'], blocked: false },
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
      { provider: 'google', linked: false, canUnlink: false, reauthWith: [], blocked: false },
      { provider: 'discord', linked: true, canUnlink: true, reauthWith: ['dev'], blocked: false },
      { provider: 'dev', linked: true, canUnlink: false, reauthWith: ['dev'], blocked: false },
    ]);
  });

  it('without the list of enabled providers, every linked one counts', () => {
    expect(providerRows(['google', 'dev'], null)).toEqual([
      {
        provider: 'google',
        linked: true,
        canUnlink: true,
        reauthWith: ['google', 'dev'],
        blocked: false,
      },
      {
        provider: 'dev',
        linked: true,
        canUnlink: true,
        reauthWith: ['google', 'dev'],
        blocked: false,
      },
    ]);
  });
});

// 009 RF18, RF20 and RF21: the web paints what reauth-status says, it never derives the rule.
const status = (over: Partial<ReauthStatus> = {}): ReauthStatus => ({
  link_ready: false,
  unlink: [
    { provider: 'google', ready: false, reauth_with: ['google'] },
    { provider: 'dev', ready: false, reauth_with: ['google', 'dev'] },
  ],
  ...over,
});

describe('providerRows with reauth-status', () => {
  const ENABLED = ['google', 'discord', 'dev'] as const;

  it('each linked row takes its reauth_with, untouched by what else is linked', () => {
    const rows = providerRows(['google', 'dev'], ENABLED, status());
    expect(rows.map((r) => [r.provider, r.reauthWith, r.blocked])).toEqual([
      ['google', ['google'], false],
      ['discord', [], false],
      ['dev', ['google', 'dev'], false],
    ]);
  });

  it('an empty reauth_with blocks the row (Area 5)', () => {
    const rows = providerRows(
      ['google', 'dev'],
      ['discord', 'dev'],
      status({
        unlink: [
          { provider: 'google', ready: false, reauth_with: [] },
          { provider: 'dev', ready: false, reauth_with: ['dev'] },
        ],
      }),
    );
    expect(rows.find((r) => r.provider === 'google')).toMatchObject({
      reauthWith: [],
      blocked: true,
    });
    expect(rows.find((r) => r.provider === 'dev')?.blocked).toBe(false);
  });

  it('without reauth-status nothing is blocked and the linked enabled providers are offered', () => {
    const rows = providerRows(['google', 'dev'], ['dev', 'discord']);
    expect(rows.find((r) => r.provider === 'google')).toMatchObject({
      reauthWith: ['dev'],
      blocked: false,
    });
  });

  it('a linked provider that reauth-status does not list falls back the same way', () => {
    // /api/me and reauth-status are two reads: a link can land between them.
    const rows = providerRows(['google', 'dev'], ENABLED, status({ unlink: [] }));
    expect(rows.find((r) => r.provider === 'google')).toMatchObject({
      reauthWith: ['google', 'dev'],
      blocked: false,
    });
  });

  it('a row that is not linked is never blocked, even if reauth-status names it', () => {
    const row = providerRows(
      ['dev'],
      ENABLED,
      status({ unlink: [{ provider: 'google', ready: false, reauth_with: [] }] }),
    ).find((r) => r.provider === 'google');
    expect(row).toMatchObject({ linked: false, reauthWith: [], blocked: false });
  });

  it('does not expose ready: no view uses it (decision 13)', () => {
    const [row] = providerRows(['google'], ['google'], status());
    expect(row).not.toHaveProperty('ready');
  });
});

describe('linkPanel', () => {
  const ENABLED = ['dev', 'discord', 'google'] as const;

  it('without link_ready and with something to link: the linked enabled, in screen order', () => {
    expect(linkPanel(status(), ['dev', 'google'], ENABLED)).toEqual(['google', 'dev']);
  });

  it('leaves out a linked provider that is not enabled', () => {
    expect(linkPanel(status(), ['discord', 'dev'], ['google', 'dev'])).toEqual(['dev']);
  });

  it('is empty with link_ready', () => {
    expect(linkPanel(status({ link_ready: true }), ['dev'], ENABLED)).toEqual([]);
  });

  it('is empty without reauth-status: the action decides', () => {
    expect(linkPanel(null, ['dev'], ENABLED)).toEqual([]);
  });

  it('is empty when nothing enabled is left to link, even without link_ready', () => {
    expect(linkPanel(status(), ['google', 'discord', 'dev'], ENABLED)).toEqual([]);
    expect(linkPanel(status(), ['dev', 'discord'], ['dev'])).toEqual([]);
  });

  it('is empty without the list of enabled providers: nothing can be linked', () => {
    expect(linkPanel(status(), ['dev'], null)).toEqual([]);
  });
});

describe('parseReauthStatus', () => {
  it('accepts the shape of the protocol', () => {
    expect(parseReauthStatus(status())).toEqual(status());
  });

  it.each([
    ['null', null],
    ['an array', []],
    ['without link_ready', { unlink: [] }],
    ['link_ready that is not a boolean', { link_ready: 'true', unlink: [] }],
    ['unlink that is not an array', { link_ready: true, unlink: {} }],
    ['an unknown provider', { link_ready: true, unlink: [unlinkEntry({ provider: 'github' })] }],
    ['ready that is not a boolean', { link_ready: true, unlink: [unlinkEntry({ ready: 1 })] }],
    ['reauth_with not a list', { link_ready: true, unlink: [unlinkEntry({ reauth_with: 'dev' })] }],
    [
      'an unknown provider in reauth_with',
      { link_ready: true, unlink: [unlinkEntry({ reauth_with: ['github'] })] },
    ],
    ['an entry that is null', { link_ready: true, unlink: [null] }],
  ])('refuses %s', (_name, value) => {
    expect(parseReauthStatus(value)).toBeNull();
  });
});

function unlinkEntry(over: Record<string, unknown>) {
  return { provider: 'dev', ready: false, reauth_with: ['dev'], ...over };
}

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
