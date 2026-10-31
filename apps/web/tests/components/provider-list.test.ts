import type { Provider, ReauthStatus } from '@torneo/protocol';
import { render } from 'svelte/server';
import { beforeAll, describe, expect, it } from 'vitest';
import ProviderList from '../../src/lib/components/account/ProviderList.svelte';
import { linkPanel, providerRows } from '../../src/lib/features/account/providers.js';
import { overwriteGetLocale } from '../../src/lib/paraglide/runtime.js';
import es from '../../messages/es.json' with { type: 'json' };

// 009 RF20 to RF22 and decision 13: the list of providers, rendered as the server sends it.
beforeAll(() => overwriteGetLocale(() => 'es'));

const ENABLED: Provider[] = ['google', 'discord', 'dev'];
const NOT_READY: ReauthStatus = {
  link_ready: false,
  unlink: [
    { provider: 'google', ready: false, reauth_with: ['dev'] },
    { provider: 'dev', ready: false, reauth_with: ['dev'] },
  ],
};
const READY: ReauthStatus = { ...NOT_READY, link_ready: true };
const REAUTH_FAIL = {
  provider: 'google',
  key: 'error_auth_reauth_required',
  code: 'auth.reauth_required',
} as const;

const escaped = (text: string) => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;');

interface Options {
  reauth?: ReauthStatus | null;
  linked?: Provider[];
  enabled?: Provider[];
  error?: typeof REAUTH_FAIL | null;
  idPrefix?: string;
}

function html(options: Options = {}): string {
  const { reauth = READY, linked = ['google', 'dev'], enabled = ENABLED, error = null } = options;
  return render(ProviderList, {
    props: {
      rows: providerRows(linked, enabled, reauth),
      linkPanel: linkPanel(reauth, linked, enabled),
      locale: 'es',
      error,
      idPrefix: options.idPrefix ?? '',
    },
  }).body;
}

/** The markup from the element with a `data-testid` to the end of its parent's closing tag. */
function section(markup: string, testid: string): string {
  const start = markup.indexOf(`data-testid="${testid}"`);
  if (start < 0) throw new Error(`no ${testid}`);
  const open = markup.lastIndexOf('<', start);
  const tag = /^<([a-z0-9]+)/.exec(markup.slice(open))?.[1] ?? '';
  const pattern = new RegExp(`<(/?)${tag}\\b`, 'g');
  pattern.lastIndex = open;
  let depth = 0;
  for (let m = pattern.exec(markup); m; m = pattern.exec(markup)) {
    depth += m[1] ? -1 : 1;
    if (depth === 0) return markup.slice(open, markup.indexOf('>', m.index) + 1);
  }
  throw new Error(`unbalanced ${testid}`);
}

const count = (text: string, part: string) => text.split(part).length - 1;
/** The ids we choose. bits-ui numbers its own per `render()`, and one page is a single render. */
const ids = (markup: string) =>
  [...markup.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]).filter((id) => !id?.startsWith('bits-'));
const DEV_FIELD = 'name="dev_identity"';

describe('a provider that nothing can confirm (Area 5)', () => {
  const status: ReauthStatus = {
    link_ready: true,
    unlink: [
      { provider: 'google', ready: false, reauth_with: [] },
      { provider: 'dev', ready: true, reauth_with: ['dev'] },
    ],
  };

  it('has no Unlink, no buttons and its explanation from the start', () => {
    const row = section(
      html({ reauth: status, enabled: ['discord', 'dev'] }),
      'provider-row-google',
    );
    expect(row).not.toContain('unlink-google');
    expect(row).not.toContain('<button');
    expect(row).toContain(escaped(es.account_unlink_disabled_hint));
  });

  it('the rows that can be confirmed keep their button', () => {
    const row = section(html({ reauth: status, enabled: ['discord', 'dev'] }), 'provider-row-dev');
    expect(row).toContain('unlink-dev');
    expect(row).not.toContain(escaped(es.account_unlink_disabled_hint));
  });
});

describe('the panel of a row, after auth.reauth_required', () => {
  const markup = () => html({ reauth: READY, error: REAUTH_FAIL });

  it('sits inside that row and only that row', () => {
    const page = markup();
    expect(section(page, 'provider-row-google')).toContain('reauth-panel-google');
    expect(section(page, 'provider-row-dev')).not.toContain('reauth-panel-');
    expect(section(page, 'reauth-panel-google')).toContain(escaped(es.account_reauth_title));
    expect(section(page, 'reauth-panel-google')).toContain(escaped(es.account_reauth_text));
  });

  it('has one form per provider of its reauth_with, with the fixed next', () => {
    const panel = section(
      html({
        reauth: {
          link_ready: true,
          unlink: [
            { provider: 'google', ready: false, reauth_with: ['google', 'dev'] },
            { provider: 'dev', ready: false, reauth_with: ['dev'] },
          ],
        },
        error: REAUTH_FAIL,
      }),
      'reauth-panel-google',
    );
    expect(count(panel, '<form')).toBe(2);
    expect(panel).toContain('action="/api/auth/reauth/google"');
    expect(panel).toContain('action="/api/auth/reauth/dev"');
    expect(count(panel, 'name="next" value="/es/account?reauth=1"')).toBe(2);
    expect(panel).toContain(escaped(es.account_reauth_with.replace('{provider}', 'Google')));
    expect(panel).toContain(
      escaped(es.account_reauth_with.replace('{provider}', es.provider_name_dev)),
    );
  });

  it('only the form of dev has the identity field', () => {
    const panel = section(
      html({
        reauth: {
          link_ready: true,
          unlink: [{ provider: 'google', ready: false, reauth_with: ['google', 'dev'] }],
        },
        error: REAUTH_FAIL,
      }),
      'reauth-panel-google',
    );
    expect(count(panel, DEV_FIELD)).toBe(1);
    const google = panel.indexOf('action="/api/auth/reauth/google"');
    const dev = panel.indexOf('action="/api/auth/reauth/dev"');
    expect(google).toBeLessThan(dev);
    expect(panel.indexOf(DEV_FIELD)).toBeGreaterThan(dev);
  });

  it('replaces the error text of its row', () => {
    expect(markup()).not.toContain('provider-error-google');
  });

  it('asks for the focus on its title, tabindex -1', () => {
    const panel = section(markup(), 'reauth-panel-google');
    expect(panel).toMatch(/<h3[^>]*tabindex="-1"/);
    expect(panel).toMatch(/<h3[^>]*autofocus/);
  });

  it('without reauth-status, offers the linked enabled providers', () => {
    const panel = section(html({ reauth: null, error: REAUTH_FAIL }), 'reauth-panel-google');
    expect(panel).toContain('action="/api/auth/reauth/google"');
    expect(panel).toContain('action="/api/auth/reauth/dev"');
    expect(panel).not.toContain('action="/api/auth/reauth/discord"');
  });

  it('another error keeps its text and opens no panel', () => {
    const page = html({
      error: {
        ...REAUTH_FAIL,
        key: 'error_auth_last_provider',
        code: 'auth.last_provider',
      } as never,
    });
    expect(page).toContain('provider-error-google');
    expect(page).not.toContain('reauth-panel-');
  });
});

describe('the panel of the link buttons', () => {
  it('without link_ready and with something to link, replaces the buttons', () => {
    const page = html({ reauth: NOT_READY });
    expect(page).toContain('reauth-panel-link');
    expect(page).not.toContain('data-testid="link-');
    const panel = section(page, 'reauth-panel-link');
    expect(panel).toContain('action="/api/auth/reauth/google"');
    expect(panel).toContain('action="/api/auth/reauth/dev"');
    expect(panel).not.toMatch(/<h3[^>]*autofocus/);
  });

  it('with link_ready, the buttons of today and no panel', () => {
    const page = html({ reauth: READY });
    expect(page).toContain('link-discord');
    expect(page).toContain('action="/api/auth/link/discord"');
    expect(page).not.toContain('reauth-panel-');
  });

  it('without reauth-status, the buttons of today and no panel', () => {
    const page = html({ reauth: null });
    expect(page).toContain('link-discord');
    expect(page).not.toContain('reauth-panel-');
  });

  it('one panel at a time: the panel of a row hides this one and the buttons', () => {
    const page = html({ reauth: NOT_READY, error: REAUTH_FAIL });
    expect(page).toContain('reauth-panel-google');
    expect(page).not.toContain('reauth-panel-link');
    expect(page).not.toContain('data-testid="link-');
    expect(count(page, DEV_FIELD)).toBe(1);
  });
});

describe('ids', () => {
  it('two lists with different prefixes repeat no id', () => {
    const blocked: ReauthStatus = {
      link_ready: false,
      unlink: [
        { provider: 'google', ready: false, reauth_with: [] },
        { provider: 'dev', ready: false, reauth_with: ['dev'] },
      ],
    };
    const options = { reauth: blocked, enabled: ['discord', 'dev'] as Provider[] };
    const all = [
      ...ids(html({ ...options, idPrefix: 'one-' })),
      ...ids(html({ ...options, idPrefix: 'two-' })),
    ];
    expect(all.length).toBeGreaterThan(2);
    expect(new Set(all).size).toBe(all.length);
  });

  it('one list repeats no id', () => {
    const all = ids(html({ reauth: NOT_READY }));
    expect(new Set(all).size).toBe(all.length);
  });
});
