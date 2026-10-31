<script lang="ts">
  import type { ReauthStatus } from '@torneo/protocol';
  import DisplayNameForm from '#lib/components/account/DisplayNameForm.svelte';
  import ProviderList from '#lib/components/account/ProviderList.svelte';
  import ReauthPanel from '#lib/components/account/ReauthPanel.svelte';
  import UserBadge from '#lib/components/account/UserBadge.svelte';
  import AuthErrorNotice from '#lib/components/auth/AuthErrorNotice.svelte';
  import DevIdentityField from '#lib/components/auth/DevIdentityField.svelte';
  import ProviderButton from '#lib/components/auth/ProviderButton.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { linkBlocked, linkPanel, providerRows } from '#lib/features/account/providers.js';
  import Section from './Section.svelte';

  // 008 plan-d decision 9: every new component and its states (demo texts exempt from i18n).
  // The forms never submit here: `/dev/components` has no actions.
  const noSubmit = (event: SubmitEvent) => event.preventDefault();

  // 009 RF21: Google switched off and older than the rest, so nothing can confirm its removal.
  const blocked: ReauthStatus = {
    link_ready: true,
    unlink: [
      { provider: 'google', ready: false, reauth_with: [] },
      { provider: 'dev', ready: true, reauth_with: ['dev'] },
    ],
  };
  // 009 RF20: without a confirmation, the panel replaces the link buttons.
  const notReady: ReauthStatus = {
    link_ready: false,
    unlink: [{ provider: 'dev', ready: false, reauth_with: ['dev'] }],
  };
  const linkOff: ReauthStatus = {
    link_ready: false,
    unlink: [{ provider: 'discord', ready: false, reauth_with: [] }],
  };
</script>

<Section title="Alert" id="alert">
  <Alert.Root>
    <Alert.Title>Default</Alert.Title>
    <Alert.Description><p>Neutral information.</p></Alert.Description>
  </Alert.Root>
  <Alert.Root variant="destructive">
    <Alert.Title>Destructive</Alert.Title>
    <Alert.Description><p>Something failed.</p></Alert.Description>
  </Alert.Root>
</Section>

<Section title="Auth" id="auth">
  <div class="grid max-w-md gap-4" data-testid="catalog-auth">
    <form class="flex flex-col gap-3" onsubmit={noSubmit}>
      <ProviderButton provider="google" />
      <ProviderButton provider="discord" />
      <ProviderButton provider="dev" disabled />
    </form>
    <form class="flex flex-col gap-3 rounded-lg border border-border p-4" onsubmit={noSubmit}>
      <DevIdentityField id="catalog-dev-identity" value="ana_bel-2" />
      <ProviderButton provider="dev" />
    </form>
    <AuthErrorNotice key="error_auth_invalid_state" title="With a title" testid="catalog-error" />
    <AuthErrorNotice key="error_generic" testid="catalog-error-generic" />
  </div>
</Section>

<Section title="Account" id="account">
  <div class="grid max-w-md gap-4">
    <div class="flex flex-wrap gap-2">
      <UserBadge name="Ana" balance={100} locale="en" href="#account" />
      <UserBadge name="Unnombremuylargoqueno cabe" balance={1234567} locale="es" href="#account" />
    </div>
    <DisplayNameForm id="catalog-name" />
    <DisplayNameForm
      id="catalog-name-error"
      value="Taken name"
      errorKey="error_profile_display_name_taken"
    />
    <ProviderList
      rows={providerRows(['dev'], ['google', 'discord', 'dev'])}
      locale="en"
      idPrefix="catalog-one-"
    />
    <ProviderList
      rows={providerRows(['google', 'dev'], ['google', 'discord', 'dev'])}
      locale="en"
      error={{ provider: 'google', key: 'error_auth_try_again' }}
      idPrefix="catalog-two-"
    />
    <ProviderList
      rows={providerRows(['google', 'dev'], ['discord', 'dev'], blocked)}
      locale="en"
      idPrefix="catalog-blocked-"
    />
    <ProviderList
      rows={providerRows(['dev'], ['google', 'dev'], notReady)}
      linkPanel={linkPanel(notReady, ['dev'], ['google', 'dev'])}
      locale="en"
      idPrefix="catalog-link-panel-"
    />
    <!-- TA-31: the only linked provider is switched off, so nothing can confirm a link. -->
    <ProviderList
      rows={providerRows(['discord'], ['google', 'dev'], linkOff)}
      linkBlocked={linkBlocked(linkOff, ['discord'], ['google', 'dev'])}
      locale="en"
      idPrefix="catalog-link-blocked-"
    />
    <ReauthPanel
      providers={['google', 'discord']}
      locale="en"
      idPrefix="catalog-panel-"
      testid="catalog-reauth-panel"
    />
    <ReauthPanel
      providers={['google', 'dev']}
      locale="es"
      idPrefix="catalog-panel-dev-"
      testid="catalog-reauth-panel-dev"
    />
  </div>
</Section>
