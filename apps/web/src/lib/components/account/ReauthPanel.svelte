<script lang="ts">
  import type { Provider } from '@torneo/protocol';
  import DevIdentityField from '#lib/components/auth/DevIdentityField.svelte';
  import { Button } from '#lib/components/ui/button/index.js';
  import type { Locale } from '#lib/i18n/locale.js';
  import { m } from '#lib/paraglide/messages.js';
  import { providerNames } from './provider-names.js';

  // The confirmation panel (009 RF22). Each form posts straight to /api: the server sets the flow
  // cookie and redirects to the provider. `next` is fixed text, never taken from the URL.
  let {
    providers,
    locale,
    idPrefix = '',
    testid,
    autofocus = false,
  }: {
    providers: readonly Provider[];
    locale: Locale;
    idPrefix?: string;
    testid: string;
    autofocus?: boolean;
  } = $props();

  let title = $state<HTMLElement | null>(null);

  // With JS, Svelte only honors `autofocus` while the focus is on `body`. After a `use:enhance`
  // action it can still be on another element, and the panel has to take it (009 RF21).
  $effect(() => {
    if (autofocus) title?.focus();
  });
</script>

<div
  class="grid gap-3 border-t border-border pt-3"
  role="group"
  aria-labelledby={`${idPrefix}reauth-title`}
  data-testid={testid}
>
  <!-- Without JS, the only way to move the focus to the panel that a 403 opened (RF21). -->
  <!-- svelte-ignore a11y_autofocus -->
  <h3
    bind:this={title}
    id={`${idPrefix}reauth-title`}
    tabindex="-1"
    {autofocus}
    class="rounded-sm font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 focus-visible:ring-offset-background"
    data-testid="reauth-title"
  >
    {m.account_reauth_title()}
  </h3>
  <p class="text-sm text-muted-foreground">{m.account_reauth_text()}</p>
  {#each providers as provider (provider)}
    <form method="post" action={`/api/auth/reauth/${provider}`} class="flex flex-col gap-3">
      <input type="hidden" name="next" value={`/${locale}/account?reauth=1`} />
      {#if provider === 'dev'}
        <DevIdentityField id={`${idPrefix}reauth-dev-identity`} />
      {/if}
      <Button
        type="submit"
        class="h-auto min-h-11 w-full whitespace-normal sm:w-auto sm:self-start"
        data-testid={`reauth-provider-${provider}`}
      >
        {m.account_reauth_with({ provider: providerNames[provider]() })}
      </Button>
    </form>
  {/each}
</div>
