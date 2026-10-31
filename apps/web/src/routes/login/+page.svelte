<script lang="ts">
  import { page } from '$app/state';
  import AuthErrorNotice from '#lib/components/auth/AuthErrorNotice.svelte';
  import DevIdentityField from '#lib/components/auth/DevIdentityField.svelte';
  import ProviderButton from '#lib/components/auth/ProviderButton.svelte';
  import * as Alert from '#lib/components/ui/alert/index.js';
  import { Button } from '#lib/components/ui/button/index.js';
  import { m } from '#lib/paraglide/messages.js';
  import { getLocale } from '#lib/paraglide/runtime.js';

  let { data } = $props();
  const locale = $derived(getLocale());
  // With a session there is nothing to sign in to, and the error did not end it (009 RF24, TA-31).
  const title = $derived(data.signedIn ? m.login_signed_in_title() : m.login_title());
</script>

<svelte:head>
  <title>{title} · {m.app_name()}</title>
</svelte:head>

<section class="mx-auto flex w-full max-w-md flex-col gap-6">
  <header class="flex flex-col gap-2">
    <h1 class="font-display text-3xl font-bold tracking-tight" data-testid="login-title">
      {title}
    </h1>
    {#if !data.signedIn}
      <p class="text-muted-foreground">{m.login_lead()}</p>
    {/if}
  </header>

  {#if data.errorKey}
    <AuthErrorNotice
      key={data.errorKey}
      title={data.signedIn ? m.login_signed_in_error_title() : m.login_error_title()}
    />
  {/if}

  {#if data.signedIn}
    <!-- After an error, going back is the main action (009 RF24). -->
    <Button
      href={`/${locale}/account`}
      variant={data.errorKey ? 'default' : 'outline'}
      class="w-full"
      data-testid="login-account-link"
    >
      {data.errorKey ? m.login_back_to_account() : m.account_link()}
    </Button>
  {:else if data.providers === null}
    <Alert.Root role="status">
      <Alert.Description>
        <p data-testid="login-providers-unavailable">{m.login_providers_unavailable()}</p>
        <a
          href={`${page.url.pathname}${page.url.search}`}
          class="inline-flex min-h-11 items-center font-medium underline underline-offset-4"
        >
          {m.login_retry()}
        </a>
      </Alert.Description>
    </Alert.Root>
  {:else if data.providers.length === 0}
    <p class="text-muted-foreground" data-testid="login-no-providers">{m.login_no_providers()}</p>
  {:else}
    <div class="flex flex-col gap-4">
      {#each data.providers as provider (provider)}
        <!-- Straight to /api: the server sets the flow cookie and redirects to the provider. -->
        <form
          method="post"
          action={`/api/auth/${provider}/start`}
          class={provider === 'dev'
            ? 'flex flex-col gap-3 rounded-lg border border-border p-4'
            : ''}
        >
          <input type="hidden" name="next" value={data.next} />
          {#if provider === 'dev'}
            <DevIdentityField />
          {/if}
          <ProviderButton {provider} />
        </form>
      {/each}
    </div>
  {/if}
</section>
