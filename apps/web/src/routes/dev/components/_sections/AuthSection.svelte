<script lang="ts">
  import DisplayNameForm from '#lib/components/account/DisplayNameForm.svelte';
  import ProviderList from '#lib/components/account/ProviderList.svelte';
  import UserBadge from '#lib/components/account/UserBadge.svelte';
  import AuthErrorNotice from '#lib/components/auth/AuthErrorNotice.svelte';
  import DevIdentityField from '#lib/components/auth/DevIdentityField.svelte';
  import ProviderButton from '#lib/components/auth/ProviderButton.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { providerRows } from '#lib/features/account/providers.js';
  import Section from './Section.svelte';

  // 008 plan-d decision 9: every new component and its states (demo texts exempt from i18n).
  // The forms never submit here: `/dev/components` has no actions.
  const noSubmit = (event: SubmitEvent) => event.preventDefault();
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
    <ProviderList rows={providerRows(['dev'], ['google', 'discord', 'dev'])} locale="en" />
    <ProviderList
      rows={providerRows(['google', 'dev'], ['google', 'discord', 'dev'])}
      locale="en"
      error={{ provider: 'google', key: 'error_auth_try_again' }}
    />
  </div>
</Section>
