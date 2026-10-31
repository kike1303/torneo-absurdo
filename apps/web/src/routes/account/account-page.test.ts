import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import AccountPage from './+page.svelte';

// TA-31: the page feeds `linkBlocked` to the list, end to end from what `load` returns.
const page = (linkReady: boolean) =>
  render(AccountPage, {
    props: {
      data: {
        state: 'ready',
        displayName: 'Ana',
        balance: 0,
        linked: ['discord'],
        enabled: ['google', 'dev'],
        reauth: {
          link_ready: linkReady,
          unlink: [{ provider: 'discord', ready: false, reauth_with: [] }],
        },
        reauthDone: false,
      },
      form: null,
    },
  } as never).body;

describe('/account, the only linked provider is switched off', () => {
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
