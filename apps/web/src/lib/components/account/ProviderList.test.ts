import { render } from 'svelte/server';
import { describe, expect, it } from 'vitest';
import type { Provider } from '@torneo/protocol';
import type { ProviderRow } from '#lib/features/account/providers.js';
import { m } from '#lib/paraglide/messages.js';
import ProviderList from './ProviderList.svelte';

const row = (provider: Provider, linked: boolean): ProviderRow => ({
  provider,
  linked,
  canUnlink: false,
  reauthWith: [],
  blocked: false,
});

function html(props: { linkPanel?: Provider[]; linkBlocked?: boolean }) {
  return render(ProviderList, {
    props: { rows: [row('google', false), row('discord', true)], locale: 'en', ...props },
  }).body;
}

// TA-31: an old session whose only linked provider is switched off (`linkBlocked` decides it).
describe('ProviderList, a link that nothing can confirm', () => {
  it('offers no link button and says why', () => {
    const body = html({ linkBlocked: true });
    expect(body).not.toContain('data-testid="link-google"');
    expect(body).toContain('data-testid="link-blocked"');
    expect(body).toContain(m.account_link_unavailable());
  });

  it('with a provider that can confirm, the panel stays and the explanation does not show', () => {
    const body = html({ linkPanel: ['dev'] });
    expect(body).toContain('data-testid="reauth-panel-link"');
    expect(body).not.toContain('data-testid="link-blocked"');
  });

  it('with a confirmation, the link buttons stay', () => {
    const body = html({});
    expect(body).toContain('data-testid="link-google"');
    expect(body).not.toContain('data-testid="link-blocked"');
  });
});
