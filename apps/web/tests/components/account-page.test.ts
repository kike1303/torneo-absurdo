import { render } from 'svelte/server';
import { beforeAll, describe, expect, it } from 'vitest';
import { overwriteGetLocale } from '../../src/lib/paraglide/runtime.js';
import AccountPage from '../../src/routes/account/+page.svelte';
import es from '../../messages/es.json' with { type: 'json' };

// 009 decision 13: what an action just did goes before "you can finish now". The UI never sends
// both, but a hand-crafted `POST /es/account?/rename&reauth=1` does: the load only canonicalizes a
// GET, so it keeps the parameter and `reauthDone` is true next to the result of the action.
beforeAll(() => overwriteGetLocale(() => 'es'));

function status(form: Record<string, unknown>): string {
  const body = render(AccountPage, {
    props: {
      data: {
        state: 'ready',
        displayName: 'Ana',
        balance: 100,
        linked: ['google'],
        enabled: ['google'],
        reauth: {
          link_ready: true,
          unlink: [{ provider: 'google', ready: true, reauth_with: ['google'] }],
        },
        reauthDone: true,
      },
      form,
    } as never,
  }).body;
  const found = /data-testid="account-status"[^>]*>([^<]*)</.exec(body);
  if (!found) throw new Error('no account-status');
  return found[1] ?? '';
}

describe('/account with reauthDone and the result of an action', () => {
  it.each([
    ['rename', { renamed: true }, es.account_renamed],
    ['unlink', { unlinked: true }, es.account_unlinked],
  ])('%s: the status is the action', (_name, form, text) => {
    expect(status(form)).toBe(text);
  });
});

// TA-31: the page hands `linkBlocked` to the list, from what `load` returns.
describe('/account, the only linked provider is switched off', () => {
  const page = (linkReady: boolean) =>
    render(AccountPage, {
      props: {
        data: {
          state: 'ready',
          displayName: 'Ana',
          balance: 100,
          linked: ['discord'],
          enabled: ['google', 'dev'],
          reauth: {
            link_ready: linkReady,
            unlink: [{ provider: 'discord', ready: false, reauth_with: [] }],
          },
          reauthDone: false,
        },
        form: null,
      } as never,
    }).body;

  it('without a confirmation: the explanation, not the link buttons', () => {
    const body = page(false);
    expect(body).toContain('data-testid="link-blocked"');
    expect(body).not.toContain('data-testid="link-google"');
  });

  it('with a confirmation: the link buttons work', () => {
    const body = page(true);
    expect(body).toContain('data-testid="link-google"');
    expect(body).not.toContain('data-testid="link-blocked"');
  });
});
